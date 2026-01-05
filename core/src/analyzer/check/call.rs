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
                    let ty_str = index_ty.display(&self.ctx.interner).to_string();
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
                let ty_str = target_ty.display(&self.ctx.interner).to_string();
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
                    params: init_params,
                    ret: Box::new(instance_type),
                };

                // A7. 转交给通用的 check_call 处理参数检查
                self.check_call(constructor_func_ty, args, callee.span)
            }

            // === Case B: 普通函数调用 ===
            // 语法: print("hello") 或 arr.push(1)
            Type::Function { .. } => {
                // 目前 Loom v0.1 的 Function 类型不存储泛型定义 (<T>)
                // 所以如果用户写 func<int>()，我们无法处理，必须报错
                if !resolved_generics.is_empty() {
                    self.report(
                        callee.span,
                        SemanticErrorKind::Custom(
                            "Generic arguments on functions are not supported yet (only on Types)"
                                .into(),
                        ),
                    );
                    return Type::Error;
                }

                self.check_call(callee_ty, args, callee.span)
            }

            // === Case C: 错误传播 ===
            Type::Error => Type::Error,

            // === Case D: 其他类型不可调用 ===
            _ => {
                let ty_str = callee_ty.display(&self.ctx.interner).to_string();
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
                        params: vec![], // 调用者不需要传参数 (self 是隐式的)
                        ret: Box::new(Type::Int),
                    }),

                    // push: (T) -> ()
                    "push" => Some(Type::Function {
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
                        params: vec![],
                        ret: Box::new(Type::Int),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // === 补全: 核心字段访问逻辑 (带泛型替换) ===

    fn check_field_access(
        &mut self,
        target_ty: Type,
        field: Symbol,
        span: crate::utils::Span,
    ) -> Type {
        // 1. 提取 Base Symbol 和 泛型实参
        // [修改] 这里需要把 Type::Module 单独拎出来处理，不要让它掉进下面的 match
        if let Type::Module(path) = &target_ty {
            // === 处理模块访问 ===

            // 1. 从 Context 中查找已加载的 ModuleInfo
            if let Some(module_info) = self.ctx.modules.get(path) {
                // 2. 在模块的导出列表 (exports) 中查找字段
                if let Some(table_info) = module_info.exports.get(&field) {
                    // [修改] 返回带有 file_id 的 Table 类型
                    // 这样 check_call_expr 就能拿到这个 ID，进而找到正确的定义
                    return Type::Table(TableId(module_info.file_id, table_info.name));
                } else {
                    // 模块加载了，但没这个名字
                    let mod_name = path
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
                    return Type::Error;
                }
            } else {
                // 理论上如果不应该发生（因为 Module 类型存在说明已经 Resolve 过了），防御性编程
                self.report(
                    span,
                    SemanticErrorKind::ModuleNotFound(format!("{:?}", path)),
                );
                return Type::Error;
            }
        }

        // 1. 提取 Base Symbol 和 泛型实参 (Args)
        let (base_sym, generic_args) = match &target_ty {
            Type::Table(sym) => (*sym, vec![]),
            Type::GenericInstance { base, args } => (*base, args.clone()),
            Type::Error => return Type::Error, // 级联错误拦截
            _ => {
                let ty_str = target_ty.display(&self.ctx.interner).to_string();
                self.report(
                    span,
                    SemanticErrorKind::Custom(format!("Type '{}' does not have fields", ty_str)),
                );
                return Type::Error;
            }
        };

        // 2. 查找 Table 定义
        let table_info = match self.find_table_info(base_sym) {
            Some(info) => info.clone(),
            None => {
                let t_name = self.ctx.resolve_symbol(base_sym.symbol()).to_string();
                self.report(span, SemanticErrorKind::UndefinedSymbol(t_name));
                return Type::Error;
            }
        };

        // 3. 构建泛型替换映射 (Substitution Map)
        let mut type_mapping = std::collections::HashMap::new();

        // 校验泛型参数数量
        if generic_args.len() != table_info.generic_params.len() {
            let t_name = self.ctx.resolve_symbol(base_sym.symbol()).to_string();

            // 如果是 Type::Table 但表其实有泛型，说明用户用了 Raw Type (如 List 而不是 List<int>)
            // 这种情况下，generic_args 为空。我们可以视作 Error，或者视为 List<Any>。
            // 这里我们严格报错：
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

        // 建立映射: T -> int, U -> str
        for (i, param_sym) in table_info.generic_params.iter().enumerate() {
            type_mapping.insert(*param_sym, generic_args[i].clone());
        }

        // 4. 查找成员 (Field or Method)

        // A. 查找字段 (Field)
        if let Some(field_ty) = table_info.fields.get(&field) {
            // [Key Step] 核心：即时替换
            // 比如定义是 item: T, mapping 是 T -> int，这里返回 int
            return field_ty.ty.substitute(&type_mapping);
        }

        // B. 查找方法 (Method)
        if let Some(method_sig) = table_info.methods.get(&field) {
            // [Key Step] 方法签名也要替换
            // 原始签名: (item: T) -> bool
            // 替换后:   (item: int) -> bool

            let new_params = method_sig
                .signature
                .params
                .iter()
                .map(|(_, p_ty)| p_ty.substitute(&type_mapping))
                .collect();

            let new_ret = method_sig.signature.ret.substitute(&type_mapping);

            // 返回一个 Function 类型
            // 注意：check_call_expr 会拿到这个 Function 类型并检查参数
            return Type::Function {
                params: new_params,
                ret: Box::new(new_ret),
            };
        }

        // 5. 未找到成员
        let t_name = self.ctx.resolve_symbol(base_sym.symbol()).to_string();
        let f_name = self.ctx.resolve_symbol(field).to_string();

        self.report(
            span,
            SemanticErrorKind::Custom(format!(
                "Property '{}' does not exist on type '{}'",
                f_name, t_name
            )),
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
            Type::Function { params, ret } => {
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
                let ty_str = func_ty.display(&self.ctx.interner).to_string();
                self.report(call_span, SemanticErrorKind::NotCallable(ty_str));
                Type::Error
            }
        }
    }
}
