use super::*;
use crate::analyzer::TableId;

impl<'a> Analyzer<'a> {
    pub(super) fn check_access_expr(&mut self, target: &Expression, field: Symbol) -> Type {
        let target_ty = self.check_expression(target);

        if let Some(builtin_ty) = self.check_builtin_member_access(&target_ty, field) {
            return builtin_ty;
        }

        self.check_field_access(target_ty, field, target.span)
    }
    pub(super) fn check_index_expr(&mut self, target: &Expression, index: &Expression) -> Type {
        let target_ty = self.check_expression(target);
        let index_ty = self.check_expression(index);

        match target_ty {
            Type::Array(inner) => {
                if index_ty != Type::Int {
                    let ty_str = index_ty.display(self.ctx).to_string();
                    self.report(
                        index.span,
                        SemanticErrorKind::InvalidIndexType(format!(
                            "Array index must be int, got {}",
                            ty_str
                        )),
                    );
                }
                *inner
            }
            Type::Str => {
                if index_ty != Type::Int {
                    self.report(
                        index.span,
                        SemanticErrorKind::InvalidIndexType("String index must be int".into()),
                    );
                }
                Type::Str
            }
            _ => {
                let ty_str = target_ty.display(self.ctx).to_string();
                self.report(target.span, SemanticErrorKind::TypeNotIndexable(ty_str));
                Type::Error
            }
        }
    }

    pub(super) fn check_call_expr(
        &mut self,
        callee: &Expression,
        generic_args: &[TypeRef],
        args: &[CallArg],
    ) -> Type {
        // 1. 检查被调用者 (callee) 的类型
        let callee_ty = self.check_expression(callee);

        // 2. 解析显式传入的泛型参数 (如果有)
        // 例如: List<int>() 中的 <int>
        let resolved_generics: Vec<Type> = generic_args
            .iter()
            .map(|g| self.resolve_ast_type(g, &std::collections::HashSet::new()))
            .collect();

        // 3. 根据 callee 类型进行分发
        match callee_ty {
            // === Case A: 构造函数调用 (Constructor) ===
            // 语法: Person("name") 或 Box<int>(10)
            Type::Table(sym) => {
                // A1. 获取 Table 元数据
                // 必须 clone，避免借用冲突
                let table_info = match self.find_table_info(sym) {
                    Some(info) => info.clone(),
                    None => {
                        let t_name = self.ctx.resolve_symbol(sym.symbol()).to_string();
                        // 这是一个很好的防御性报错，万一 Table 类型传递过来了但定义丢了
                        self.report(
                            callee.span,
                            SemanticErrorKind::Custom(format!(
                                "Definition for table '{}' not found",
                                t_name
                            )),
                        );
                        return Type::Error;
                    }
                };

                // A2. 检查泛型参数数量
                // 定义: table Box<T>
                // 调用: Box<int>() -> OK
                // 调用: Box()      -> Error
                if resolved_generics.len() != table_info.generic_params.len() {
                    let t_name = self.ctx.resolve_symbol(sym.symbol()).to_string();
                    self.report(
                        callee.span,
                        SemanticErrorKind::GenericArgumentCountMismatch {
                            name: t_name,
                            expected: table_info.generic_params.len(),
                            found: resolved_generics.len(),
                        },
                    );
                    return Type::Error;
                }

                // A3. 构建“实例类型” (Instance Type)
                // 这是这个调用最终返回的类型
                let instance_type = if resolved_generics.is_empty() {
                    Type::Table(sym)
                } else {
                    Type::GenericInstance {
                        base: sym,
                        args: resolved_generics.clone(),
                    }
                };

                // A4. 准备泛型替换映射 (Substitution Map)
                // 用于把 init(val: T) 变成 init(val: int)
                let mut type_mapping = std::collections::HashMap::new();
                for (i, param_sym) in table_info.generic_params.iter().enumerate() {
                    type_mapping.insert(*param_sym, resolved_generics[i].clone());
                }

                // A5. 查找构造方法 `init`
                // 默认构造函数：如果没有定义 init，则期望 0 个参数
                let init_params =
                    if let Some(init_sig) = table_info.methods.get(&self.ctx.intern("init")) {
                        // 对 init 方法的参数进行泛型替换
                        init_sig
                            .signature
                            .params
                            .iter()
                            .map(|(_, ty)| ty.substitute(&type_mapping))
                            .collect() // Vec<Type>
                    } else {
                        vec![] // 无 init 方法，默认无参构造
                    };

                // A6. 构造一个“合成”的函数类型用于检查
                // 参数：来自 init
                // 返回值：来自 instance_type (这是关键！构造函数返回实例，而不是 init 的 void)
                let constructor_func_ty = Type::Function {
                    generic_params: vec![],
                    params: init_params,
                    ret: Box::new(instance_type),
                };

                // A7. 转交给通用的 check_call 处理参数检查
                self.check_call(constructor_func_ty, args, callee.span)
            }

            // === Case B: 普通函数调用 ===
            // 语法: print("hello") 或 arr.push(1)
            Type::Function {
                generic_params,
                params,
                ret,
            } => {
                // 情况 1: 用户提供了泛型参数 func<int>()
                if !resolved_generics.is_empty() {
                    // 1.1 检查泛型参数数量
                    if resolved_generics.len() != generic_params.len() {
                        self.report(
                            callee.span,
                            SemanticErrorKind::GenericArgumentCountMismatch {
                                name: "<function>".into(),
                                expected: generic_params.len(),
                                found: resolved_generics.len(),
                            },
                        );
                        return Type::Error;
                    }

                    // 1.2 构建替换表 { T -> int }
                    let mut type_mapping = std::collections::HashMap::new();
                    for (i, param_sym) in generic_params.iter().enumerate() {
                        type_mapping.insert(*param_sym, resolved_generics[i].clone());
                    }

                    // 1.3 实例化：替换参数和返回值类型
                    let instantiated_params: Vec<Type> =
                        params.iter().map(|p| p.substitute(&type_mapping)).collect();

                    let instantiated_ret = ret.substitute(&type_mapping);

                    // 1.4 构造一个新的实例化后的函数类型用于检查 (无泛型了)
                    let func_instance = Type::Function {
                        generic_params: vec![], // 已实例化，清空泛型列表
                        params: instantiated_params,
                        ret: Box::new(instantiated_ret),
                    };

                    // 1.5 转交到底层检查
                    return self.check_call(func_instance, args, callee.span);
                }

                // 情况 2: 用户没提供泛型参数 func()
                // 如果函数本身是泛型的 (generic_params 不为空)，这通常是不允许的（除非支持推导）
                if !generic_params.is_empty() {
                    // 简单起见，v0.2 要求显式泛型
                    self.report(
                        callee.span,
                        SemanticErrorKind::Custom(
                            "Generic function requires explicit type arguments (e.g. func<int>(...))".into()
                        )
                     );
                    return Type::Error;
                }

                // 情况 3: 普通非泛型函数
                let func_ty = Type::Function {
                    generic_params,
                    params,
                    ret,
                };
                self.check_call(func_ty, args, callee.span)
            }

            // === Case C: 错误传播 ===
            Type::Error => Type::Error,

            // === Case D: 其他类型不可调用 ===
            _ => {
                let ty_str = callee_ty.display(self.ctx).to_string();
                self.report(callee.span, SemanticErrorKind::NotCallable(ty_str));
                Type::Error
            }
        }
    }

    fn check_builtin_member_access(&mut self, target_ty: &Type, field: Symbol) -> Option<Type> {
        let field_name = self.ctx.resolve_symbol(field);

        match target_ty {
            // 数组的内置成员
            Type::Array(inner) => {
                match field_name {
                    // [修改] len 现在是一个函数: () -> int
                    "len" => Some(Type::Function {
                        generic_params: vec![],
                        params: vec![], // 调用者不需要传参数 (self 是隐式的)
                        ret: Box::new(Type::Int),
                    }),

                    // push: (T) -> ()
                    "push" => Some(Type::Function {
                        generic_params: vec![],
                        params: vec![*inner.clone()],
                        ret: Box::new(Type::Unit),
                    }),
                    _ => None,
                }
            }
            // 字符串的内置成员
            Type::Str => {
                match field_name {
                    // [修改] len 现在是一个函数: () -> int
                    "len" => Some(Type::Function {
                        generic_params: vec![],
                        params: vec![],
                        ret: Box::new(Type::Int),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // [Refactor] 主入口：分发检查逻辑
    pub(super) fn check_field_access(
        &mut self,
        target_ty: Type,
        field: Symbol,
        span: crate::utils::Span,
    ) -> Type {
        match target_ty {
            // 1. 模块访问: import math; math.pi
            // [Fix] 现在持有 FileId
            Type::Module(file_id) => self.check_module_member_access(file_id, field, span),

            // 2. 实例/泛型访问: obj.field
            Type::Table(table_id) => {
                self.check_instance_member_access(table_id, vec![], field, span)
            }
            Type::GenericInstance { base, args } => {
                self.check_instance_member_access(base, args, field, span)
            }

            // 3. 错误传播
            Type::Error => Type::Error,

            // 4. 不支持的类型
            _ => {
                let ty_str = target_ty.display(self.ctx).to_string();
                self.report(
                    span,
                    SemanticErrorKind::Custom(format!("Type '{}' does not have fields", ty_str)),
                );
                Type::Error
            }
        }
    }

    /// [New] 辅助函数：处理模块成员访问
    fn check_module_member_access(
        &mut self,
        file_id: crate::source::FileId, // [Fix] 输入是 FileId
        field: Symbol,
        span: crate::utils::Span,
    ) -> Type {
        // 1. 获取文件路径
        // 我们需要路径来查 ctx.modules (HashMap<PathBuf, ModuleInfo>)
        // 注意：这里假设 source_manager 能拿到 Path
        let file_path = match self.ctx.source_manager.get_file_path(file_id) {
            Some(p) => p,
            None => {
                self.report(
                    span,
                    SemanticErrorKind::FileIOError("Invalid FileID in Module Type".into()),
                );
                return Type::Error;
            }
        };

        // 2. 查找已加载的 ModuleInfo
        let module_info = match self.ctx.modules.get(file_path) {
            Some(info) => info,
            None => {
                self.report(
                    span,
                    SemanticErrorKind::ModuleNotFound(format!("{:?}", file_path)),
                );
                return Type::Error;
            }
        };

        // 3. 依次查找导出成员

        // A. 查找导出的类 (Tables)
        // [Fix] 构造目标 TableId: (模块的文件ID, 类名)
        let target_id = TableId(module_info.file_id, field);

        // 使用构造好的 ID 去 module_info.tables 里查找
        if let Some(_table_info) = module_info.tables.get(&target_id) {
            // 找到了！返回这个 ID 对应的类型
            return Type::Table(target_id);
        }

        // B. 查找导出的顶层函数 (Functions)
        if let Some(func_info) = module_info.functions.get(&field) {
            let params: Vec<Type> = func_info
                .signature
                .params
                .iter()
                .map(|(_, ty)| ty.clone())
                .collect();

            return Type::Function {
                generic_params: func_info.generic_params.clone(),
                params,
                ret: Box::new(func_info.signature.ret.clone()),
            };
        }

        // C. 查找导出的顶层变量 (Globals)
        if let Some(global_info) = module_info.globals.get(&field) {
            return global_info.ty.clone();
        }

        // 4. 报错
        let mod_name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");
        let f_name = self.ctx.resolve_symbol(field).to_string();

        self.report(
            span,
            SemanticErrorKind::UndefinedSymbol(format!(
                "Export '{}' not found in module '{}'",
                f_name, mod_name
            )),
        );
        Type::Error
    }

    /// [New] 辅助函数：处理实例/对象成员访问
    fn check_instance_member_access(
        &mut self,
        base_table_id: TableId,
        generic_args: Vec<Type>,
        field: Symbol,
        span: crate::utils::Span,
    ) -> Type {
        // 1. 查找 Table 定义
        // find_table_info 应该支持跨文件查找 (根据 TableId 里的 file_id)
        let table_info = match self.find_table_info(base_table_id) {
            Some(info) => info.clone(),
            None => return Type::Error,
        };

        // 2. 构建替换表 (Substitution Map)
        let mut type_mapping = std::collections::HashMap::new();

        if generic_args.len() == table_info.generic_params.len() {
            for (i, param_sym) in table_info.generic_params.iter().enumerate() {
                type_mapping.insert(*param_sym, generic_args[i].clone());
            }
        } else {
            let t_name = self.ctx.resolve_symbol(base_table_id.symbol()).to_string();
            self.report(
                span,
                SemanticErrorKind::GenericArgumentCountMismatch {
                    name: t_name,
                    expected: table_info.generic_params.len(),
                    found: generic_args.len(),
                },
            );
            return Type::Error;
        }

        // 3. 查找成员并应用替换

        // Case A: 查找字段
        if let Some(field_info) = table_info.fields.get(&field) {
            return field_info.ty.substitute(&type_mapping);
        }

        // Case B: 查找方法
        if let Some(method_info) = table_info.methods.get(&field) {
            let sig = &method_info.signature;

            // 替换参数类型 (应用类泛型 T 的替换，例如 Box<int> 把 T 换成 int)
            let new_params = sig
                .params
                .iter()
                .map(|(_, ty)| ty.substitute(&type_mapping))
                .collect();

            // 替换返回值类型
            let new_ret = sig.ret.substitute(&type_mapping);

            return Type::Function {
                // [Fix] 必须保留方法自己的泛型定义！
                // 例如: [Box<T>] map<U>(f: Function<T, U>) -> Box<U>
                // 这里 T 被替换了，但 U 还是方法的泛型，必须保留
                generic_params: method_info.generic_params.clone(),
                params: new_params,
                ret: Box::new(new_ret),
            };
        }

        // 4. 没找到
        let f_name = self.ctx.resolve_symbol(field).to_string();
        self.report(
            span,
            SemanticErrorKind::Custom(format!("Member '{}' not found", f_name)),
        );
        Type::Error
    }

    fn check_call(
        &mut self,
        func_ty: Type,
        args: &[CallArg],
        call_span: crate::utils::Span,
    ) -> Type {
        match func_ty {
            Type::Function { params, ret, .. } => {
                // 1. 检查参数数量
                if args.len() != params.len() {
                    self.report(
                        call_span,
                        SemanticErrorKind::ArgumentCountMismatch {
                            func_name: "<function>".to_string(),
                            expected: params.len(),
                            found: args.len(),
                        },
                    );
                    return Type::Error;
                }

                // 2. 检查参数类型
                for (i, arg) in args.iter().enumerate() {
                    let arg_ty = self.check_expression(&arg.value);
                    // 注意：假设 params 是 Vec<Type>。
                    // 如果你的 Type 定义里 params 是 Vec<(Symbol, Type)>，这里要写 params[i].1
                    let expected_ty = &params[i];

                    // [核心修改]
                    // 旧代码: if !expected_ty.is_assignable_from(&arg_ty) {
                    // 新代码: 调用 Analyzer 的增强检查（支持继承查表）
                    if !self.check_type_compatibility(expected_ty, &arg_ty) {
                        // 使用参数自己的 span 报错
                        self.error_type_mismatch(arg.value.span, expected_ty, &arg_ty);
                    }
                }

                // 返回函数的返回类型
                *ret
            }

            Type::Error => Type::Error,

            _ => {
                let ty_str = func_ty.display(self.ctx).to_string();
                self.report(call_span, SemanticErrorKind::NotCallable(ty_str));
                Type::Error
            }
        }
    }
}
