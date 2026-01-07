use super::*;
use crate::analyzer::TableId;

impl<'a> Analyzer<'a> {
    /// [Refactor] 检查 Table 定义 (拆分版)
    pub(super) fn check_table_definition(&mut self, def: &TableDefinition) {
        // 1. 构造 TableId (当前文件 + 类名)
        let table_id = TableId(self.current_file_id, def.name);

        // 获取 TableInfo (Clone 以避免借用冲突)
        let table_info = match self.tables.get(&table_id) {
            Some(info) => info.clone(),
            None => return, // Should trigger internal error
        };

        // 2. 检查继承约束 (Override)
        self.check_inheritance_rules(&table_info);

        // 3. 检查字段初始化与推导
        self.check_field_initializations(def, &table_info, table_id);

        // 4. 检查方法体
        for item in &def.items {
            if let TableItem::Method(method_def) = item {
                // check_method_body 内部会处理作用域
                self.check_method_body(method_def, &table_info);
            }
        }

        // 5. 完整性检查 (Abstract Implementation)
        self.check_abstract_implementation(&table_info, def.span);
    }

    /// 辅助：检查继承规则
    fn check_inheritance_rules(&mut self, table_info: &crate::analyzer::info::TableInfo) {
        if let Some(parent_type) = &table_info.parent {
            // 提取父类 ID
            let parent_id = match parent_type {
                Type::Table(id) => *id,
                Type::GenericInstance { base, .. } => *base,
                _ => return,
            };

            // [Fix] 使用 find_table_info 支持跨模块查找父类
            if let Some(parent_info) = self.find_table_info(parent_id) {
                self.check_override_constraints(table_info, &parent_info);
            }
        }
    }

    /// 辅助：检查字段初始化
    fn check_field_initializations(
        &mut self,
        def: &TableDefinition,
        table_info: &crate::analyzer::info::TableInfo,
        table_id: TableId,
    ) {
        let mut updates = HashMap::new();

        for item in &def.items {
            if let TableItem::Field(field_def) = item {
                self.scopes.enter_scope();

                if let Some(init_expr) = &field_def.value {
                    let expr_type = self.check_expression(init_expr);

                    if let Some(current_field_info) = table_info.fields.get(&field_def.name) {
                        if current_field_info.ty == Type::Infer {
                            // [Inference] 推导
                            if expr_type != Type::Error {
                                updates.insert(field_def.name, expr_type);
                            }
                        } else {
                            // [Check] 检查类型兼容性
                            // 直接使用 check_type_compatibility，它包含了继承检查(is_subtype)
                            if !self.check_type_compatibility(&current_field_info.ty, &expr_type) {
                                let f_name = self.ctx.resolve_symbol(field_def.name).to_string();
                                // 记得传入 self.ctx
                                let exp_str = current_field_info.ty.display(self.ctx).to_string();
                                let got_str = expr_type.display(self.ctx).to_string();

                                self.report(
                                    field_def.span,
                                    SemanticErrorKind::FieldTypeMismatch {
                                        field: f_name,
                                        expected: exp_str,
                                        found: got_str,
                                    },
                                );
                            }
                        }
                    }
                }
                self.scopes.exit_scope();
            }
        }

        // 应用推导结果
        if !updates.is_empty() {
            if let Some(info) = self.tables.get_mut(&table_id) {
                for (name, new_ty) in updates {
                    if let Some(field_info) = info.fields.get_mut(&name) {
                        field_info.ty = new_ty;
                    }
                }
            }
        }
    }

    // [New] 检查顶层变量定义 (例如: count: int = 10)
    pub(super) fn check_top_level_field(&mut self, def: &FieldDefinition) {
        // 1. 如果有初始值，先检查表达式的类型
        if let Some(expr) = &def.value {
            let expr_ty = self.check_expression(expr);

            // 2. 获取该变量在 Global 表中的信息
            // 注意：我们要修改它，所以需要 get_mut
            if let Some(global_info) = self.globals.get_mut(&def.name) {
                // Case A: 变量类型是 Infer (未显式标注: count = 10)
                if global_info.ty == Type::Infer {
                    if expr_ty != Type::Error {
                        // [Step 1] 更新 Global 表 (供其他模块 import 使用)
                        global_info.ty = expr_ty.clone();

                        // [Step 2] 更新当前作用域 (供当前文件的后续代码使用)
                        let _ = self.scopes.define(
                            def.name,
                            expr_ty,
                            SymbolKind::Variable,
                            def.span,
                            self.current_file_id,
                            true,
                        );
                    }
                }
                // Case B: 变量有显式标注 (count: int = "hello")
                else {
                    if !global_info.ty.is_assignable_from(&expr_ty) {
                        let expected_str = global_info.ty.display(&self.ctx).to_string();
                        let got_str = expr_ty.display(&self.ctx).to_string();

                        self.report(
                            def.span,
                            SemanticErrorKind::FieldTypeMismatch {
                                field: self.ctx.resolve_symbol(def.name).to_string(),
                                expected: expected_str,
                                found: got_str,
                            },
                        );
                    }
                }
            }
        }
    }

    /// 检查是否遗留了未实现的抽象方法
    pub(super) fn check_abstract_implementation(
        &mut self,
        info: &TableInfo,
        span: crate::utils::Span,
    ) {
        for (name, sig) in &info.methods {
            if sig.signature.is_abstract {
                let m_name = self.ctx.resolve_symbol(*name).to_string();
                let t_name = self.ctx.resolve_symbol(info.name).to_string();

                self.report(
                    span,
                    SemanticErrorKind::MissingAbstractImplementation {
                        table: t_name,
                        method: m_name,
                    },
                );
            }
        }
    }

    /// 验证子类是否遵守了父类的契约
    /// 验证子类是否遵守了父类的契约
    pub(super) fn check_override_constraints(&mut self, child: &TableInfo, parent: &TableInfo) {
        // 1. 构建泛型替换映射
        let mut type_mapping = HashMap::new();

        if let Some(parent_ref_ty) = &child.parent {
            if let Type::GenericInstance { args, .. } = parent_ref_ty {
                if args.len() == parent.generic_params.len() {
                    for (i, param_sym) in parent.generic_params.iter().enumerate() {
                        type_mapping.insert(*param_sym, args[i].clone());
                    }
                }
            }
        }

        // 2. 检查字段覆盖兼容性
        // 注意：child_info 现在是 FieldInfo，包含 .ty 和 .span
        for (name, child_info) in &child.fields {
            if let Some(raw_parent_info) = parent.fields.get(name) {
                // 父类字段类型需要替换泛型 (例如 Base<T> -> Base<int>)
                let expected_ty = raw_parent_info.ty.substitute(&type_mapping);

                // 检查：子类字段必须能够“装下”父类字段的要求
                // 通常字段类型必须是不变的 (Invariant) 或者是协变的 (Covariant，如果是只读)
                // 这里我们使用 is_assignable_from，意味着允许协变 (Parent = Child 是合法的)
                if !expected_ty.is_assignable_from(&child_info.ty) {
                    let f_name = self.ctx.resolve_symbol(*name).to_string();
                    let child_ty_str = child_info.ty.display(&self.ctx).to_string();
                    let parent_ty_str = expected_ty.display(&self.ctx).to_string();
                    let parent_name = self.ctx.resolve_symbol(parent.name).to_string();

                    self.report(
                        child_info.span, // [Fix] 使用字段自己的 Span
                        SemanticErrorKind::ConstraintViolation {
                            field: f_name,
                            reason: format!(
                                "type '{}' is not assignable to parent '{}' type '{}'",
                                child_ty_str, parent_name, parent_ty_str
                            ),
                        },
                    );
                }
            }
        }

        // 3. 检查方法覆盖兼容性
        // 注意：child_info 现在是 MethodInfo，包含 .signature 和 .span
        for (name, child_info) in &child.methods {
            if let Some(raw_parent_info) = parent.methods.get(name) {
                // 替换父类签名中的泛型
                let expected_params: Vec<Type> = raw_parent_info
                    .signature
                    .params
                    .iter()
                    .map(|(_, t)| t.substitute(&type_mapping))
                    .collect();
                let expected_ret = raw_parent_info.signature.ret.substitute(&type_mapping);

                // A. [Fix] 参数数量检查
                if child_info.signature.params.len() != expected_params.len() {
                    let m_name = self.ctx.resolve_symbol(*name).to_string();
                    self.report(
                        child_info.span, // [Fix] 使用方法自己的 Span
                        SemanticErrorKind::MethodOverrideMismatch {
                            method: m_name,
                            reason: format!(
                                "parameter count mismatch: expected {}, found {}",
                                expected_params.len(),
                                child_info.signature.params.len()
                            ),
                        },
                    );
                    continue; // 数量不对，后续类型没法对应检查，直接跳过
                }

                // B. 参数类型检查：逆变 (Contravariance)
                // 规则：子类参数必须比父类“更宽泛”或相同
                // ChildParam.is_assignable_from(ParentParam) => True
                for (i, (_, c_p_ty)) in child_info.signature.params.iter().enumerate() {
                    let e_p_ty = &expected_params[i]; // Parent (Expected) Param

                    if !c_p_ty.is_assignable_from(e_p_ty) {
                        let m_name = self.ctx.resolve_symbol(*name).to_string();
                        let c_str = c_p_ty.display(&self.ctx).to_string();
                        let e_str = e_p_ty.display(&self.ctx).to_string();

                        self.report(
                            child_info.span, // [Fix] 使用方法自己的 Span
                            SemanticErrorKind::MethodOverrideMismatch {
                                method: m_name,
                                reason: format!(
                                    "parameter {} type mismatch: child expects '{}', which is not a supertype of parent expectation '{}' (Contravariance violation)",
                                    i + 1, // 友好的 1-based index
                                    c_str, e_str
                                ),
                            }
                        );
                    }
                }

                // C. 返回值类型检查：协变 (Covariance)
                // 规则：子类返回值必须比父类“更具体”或相同
                // ParentRet.is_assignable_from(ChildRet) => True
                if !expected_ret.is_assignable_from(&child_info.signature.ret) {
                    let m_name = self.ctx.resolve_symbol(*name).to_string();
                    let c_str = child_info.signature.ret.display(&self.ctx).to_string();
                    let e_str = expected_ret.display(&self.ctx).to_string();

                    self.report(
                        child_info.span, // [Fix] 使用方法自己的 Span
                        SemanticErrorKind::MethodOverrideMismatch {
                            method: m_name,
                            reason: format!(
                                "return type mismatch: child returns '{}', which is not a subtype of parent return '{}' (Covariance violation)",
                                c_str, e_str
                            ),
                        }
                    );
                }
            }
        }
    }

    pub(super) fn check_method_body(
        &mut self,
        method: &MethodDefinition,
        current_table: &TableInfo,
    ) {
        let body_block = match &method.body {
            Some(b) => b,
            None => return, // 抽象方法没有体
        };

        // 保存之前的返回类型状态 (因为可能会嵌套定义函数? 虽然 Loom 目前不支持局部函数，但这是一个好习惯)
        let prev_return_type = self.current_return_type.clone();

        // [Fix] 获取方法签名信息
        // 注意：这里 sig_info 是 &MethodInfo (包含 span 和 signature)
        // 我们借用 signature 而不是 move 它
        let sig_info = current_table.methods.get(&method.name).unwrap();
        let sig = &sig_info.signature;

        let expected_ret = sig.ret.clone();
        self.current_return_type = Some(expected_ret.clone());

        self.scopes.enter_scope();

        // 1. 定义 `self`
        // ---------------------------------------------------------
        // 构造当前 Table 的唯一 ID
        let table_id = TableId(current_table.file_id, current_table.name);

        // 构造 self 的类型 (处理泛型)
        let self_type = if !current_table.generic_params.is_empty() {
            let args = current_table
                .generic_params
                .iter()
                .map(|s| Type::GenericParam(*s))
                .collect();

            Type::GenericInstance {
                base: table_id,
                args,
            }
        } else {
            Type::Table(table_id)
        };

        // [New] 定义 self
        // span: 使用 method.span。这意味着在 IDE 里如果你 hover `self`，它可能会高亮整个方法定义或方法名，这是合理的。
        let _ = self.scopes.define(
            self.ctx.intern("self"),
            self_type,
            SymbolKind::Variable, // 或者你可以加一个 SymbolKind::Self
            method.span,          // <--- 1. 定义位置：当前方法的 Span
            self.current_file_id, // <--- 2. 定义文件
            false,
        );

        // 2. 定义参数
        // ---------------------------------------------------------
        for param in &method.params {
            // 从签名中查找已经 Resolve 好的类型
            if let Some((_, p_ty)) = sig.params.iter().find(|(n, _)| *n == param.name) {
                // [New] 定义参数符号
                let _ = self.scopes.define(
                    param.name,
                    p_ty.clone(),
                    SymbolKind::Parameter,
                    param.span,           // <--- 1. 定义位置：参数节点本身的 Span (x: int)
                    self.current_file_id, // <--- 2. 定义文件
                    false,
                );
            }
        }

        // 3. 检查 Body
        let body_type = self.check_block(body_block);

        // 4. 检查返回值
        if !expected_ret.is_assignable_from(&body_type) {
            // 如果期望 Unit，允许隐式返回
            if expected_ret != Type::Unit {
                self.error_type_mismatch(
                    method.span, // 这里如果有 body_block.span 会更好，但 method.span 也行
                    &expected_ret,
                    &body_type,
                );
            }
        }

        self.scopes.exit_scope();
        self.current_return_type = prev_return_type;
    }

    // [Refactor] 这是一个通用的函数体检查器
    // 既可以用于方法 (传入 Some(table))，也可以用于顶层函数 (传入 None)
    pub(super) fn check_function_like_body(
        &mut self,
        func_def: &MethodDefinition,
        parent_table: Option<&TableInfo>, // 如果是 None，说明是顶层函数
    ) {
        let body_block = match &func_def.body {
            Some(b) => b,
            None => return, // 抽象方法直接返回
        };

        // 1. 获取期望的返回值类型 & 参数列表
        let (expected_ret, params_info) = if let Some(table) = parent_table {
            // Case A: 是方法 -> 去 TableInfo 里找
            let m_info = table.methods.get(&func_def.name).unwrap();
            (m_info.signature.ret.clone(), &m_info.signature.params)
        } else {
            // Case B: 是顶层函数 -> 去 FunctionInfo 里找
            let f_info = self.functions.get(&func_def.name).unwrap();
            (f_info.signature.ret.clone(), &f_info.signature.params)
        };

        // 2. 设置上下文 (用于 check_return)
        let prev_return_type = self.current_return_type.clone();
        self.current_return_type = Some(expected_ret.clone());

        self.scopes.enter_scope();

        // 3. 如果是方法，定义 `self`
        if let Some(table) = parent_table {
            let table_id = TableId(table.file_id, table.name);

            // 处理类泛型 Box<T>
            let self_type = if !table.generic_params.is_empty() {
                let args = table
                    .generic_params
                    .iter()
                    .map(|s| Type::GenericParam(*s))
                    .collect();
                Type::GenericInstance {
                    base: table_id,
                    args,
                }
            } else {
                Type::Table(table_id)
            };

            let _ = self.scopes.define(
                self.ctx.intern("self"),
                self_type,
                SymbolKind::Variable,
                func_def.span,
                self.current_file_id,
                false,
            );
        }

        // 4. 定义参数 (通用逻辑)
        // 遍历 AST 的参数，从 Info 里拿到已经解析好的 Type
        for param in &func_def.params {
            if let Some((_, p_ty)) = params_info.iter().find(|(n, _)| *n == param.name) {
                let _ = self.scopes.define(
                    param.name,
                    p_ty.clone(),
                    SymbolKind::Parameter,
                    param.span,
                    self.current_file_id,
                    false,
                );
            }
        }

        // 5. 检查函数体 Block
        let body_type = self.check_block(body_block);

        // 6. 检查隐式返回值 (Block 的最后一个表达式)
        if !expected_ret.is_assignable_from(&body_type) {
            // 除非期望 Unit，否则必须匹配
            if expected_ret != Type::Unit {
                // 如果 body 是 Never (比如里面全是 return)，那也是合法的
                if body_type != Type::Never {
                    self.error_type_mismatch(func_def.span, &expected_ret, &body_type);
                }
            }
        }

        self.scopes.exit_scope();
        self.current_return_type = prev_return_type;
    }
}
