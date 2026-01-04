mod expr; // expr.rs 的逻辑在下一轮实现，这里只负责入口和 Table 级检查

use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::{Analyzer, SymbolKind, TableInfo, Type};
use crate::ast::*;
use crate::utils::Symbol;
use std::collections::HashMap;

impl<'a> Analyzer<'a> {
    /// Pass 3 入口：类型检查与约束验证
    pub fn check_program(&mut self, program: &Program) {
        for item in &program.definitions {
            if let TopLevelItem::Table(def) = item {
                self.check_table_definition(def);
            }
        }
    }

    fn check_table_definition(&mut self, def: &TableDefinition) {
        let table_name = def.name;
        // 1. 获取 Resolve 阶段生成的完整 TableInfo
        // 使用 clone 避免借用冲突
        let table_info = match self.tables.get(&table_name) {
            Some(info) => info.clone(),
            None => return, // 应该在 Collect 阶段就处理了
        };

        // 2. 验证 Override 约束
        if let Some(parent_type) = &table_info.parent {
            if let Some(parent_sym) = parent_type.get_base_symbol() {
                if let Some(parent_info) = self.tables.get(&parent_sym).cloned() {
                    self.check_override_constraints(&table_info, &parent_info, def.span);
                }
            }
        }

        // 3. 检查字段初始化表达式
        let mut updates = HashMap::new();

        for item in &def.items {
            if let TableItem::Field(field_def) = item {
                self.scopes.enter_scope(); // Field init 作用域

                if let Some(init_expr) = &field_def.value {
                    let expr_type = self.check_expression(init_expr);
                    let current_field_type = table_info.fields.get(&field_def.name).unwrap();

                    if *current_field_type == Type::Infer {
                        // 推导：更新字段类型
                        if expr_type != Type::Error {
                            updates.insert(field_def.name, expr_type);
                        }
                    } else {
                        // 检查：验证类型匹配
                        if !current_field_type.is_assignable_from(&expr_type) {
                            let f_name = self.ctx.resolve_symbol(field_def.name).to_string();
                            let exp_str =
                                current_field_type.display(&self.ctx.interner).to_string();
                            let got_str = expr_type.display(&self.ctx.interner).to_string();

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
                self.scopes.exit_scope();
            }
        }

        // 4. 应用推导结果
        if !updates.is_empty() {
            let info = self.tables.get_mut(&table_name).unwrap();
            for (name, ty) in updates {
                info.fields.insert(name, ty);
            }
        }

        // 5. 检查方法体
        for item in &def.items {
            if let TableItem::Method(method_def) = item {
                self.check_method_body(method_def, &table_info);
            }
        }

        // 6. 完整性检查 (Abstract Implementation)
        self.check_abstract_implementation(&table_info, def.span);
    }

    /// 检查是否遗留了未实现的抽象方法
    fn check_abstract_implementation(&mut self, info: &TableInfo, span: crate::utils::Span) {
        for (name, sig) in &info.methods {
            if sig.is_abstract {
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
    fn check_override_constraints(
        &mut self,
        child: &TableInfo,
        parent: &TableInfo,
        span: crate::utils::Span, // 传入 Table 定义的 Span 作为默认报错位置
    ) {
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
        for (name, child_ty) in &child.fields {
            if let Some(raw_parent_ty) = parent.fields.get(name) {
                let expected_ty = raw_parent_ty.substitute(&type_mapping);

                if !expected_ty.is_assignable_from(child_ty) {
                    let f_name = self.ctx.resolve_symbol(*name).to_string();
                    let child_ty_str = child_ty.display(&self.ctx.interner).to_string();
                    let parent_ty_str = expected_ty.display(&self.ctx.interner).to_string();
                    let parent_name = self.ctx.resolve_symbol(parent.name).to_string();

                    self.report(
                        span, // 这里如果有 Field 定义的 Span 更好，没有就用 Table 的
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
        for (name, child_sig) in &child.methods {
            if let Some(raw_parent_sig) = parent.methods.get(name) {
                // 替换父类签名中的泛型
                let expected_params: Vec<Type> = raw_parent_sig
                    .params
                    .iter()
                    .map(|(_, t)| t.substitute(&type_mapping))
                    .collect();
                let expected_ret = raw_parent_sig.ret.substitute(&type_mapping);

                // A. 参数数量 (保持不变)
                if child_sig.params.len() != expected_params.len() {
                    // ... 报错: SemanticErrorKind::MethodOverrideMismatch ...
                    continue;
                }

                // B. [Fix] 参数类型检查：逆变 (Contravariance)
                // 父类: fn eat(food: Dog)
                // 子类: fn eat(food: Animal) -> 合法！(Animal > Dog)
                // 规则：ChildParam.is_assignable_from(ParentParam)
                for (i, (_, c_p_ty)) in child_sig.params.iter().enumerate() {
                    let e_p_ty = &expected_params[i]; // Parent (Expected) Param

                    // 之前的错误写法: if c_p_ty != e_p_ty { ... }

                    // 正确写法：子类参数必须能兼容父类参数
                    // 即：子类参数必须宽于或等于父类参数
                    if !c_p_ty.is_assignable_from(e_p_ty) {
                        let m_name = self.ctx.resolve_symbol(*name).to_string();
                        let c_str = c_p_ty.display(&self.ctx.interner).to_string();
                        let e_str = e_p_ty.display(&self.ctx.interner).to_string();

                        self.report(
                            span,
                            SemanticErrorKind::MethodOverrideMismatch {
                                method: m_name,
                                reason: format!(
                                    "parameter {} type mismatch: child expects '{}', which is not a supertype of parent expectation '{}' (Contravariance violation)",
                                    i, c_str, e_str
                                ),
                            }
                        );
                    }
                }

                // C. [Fix] 返回值类型检查：协变 (Covariance)
                // 父类: fn get() -> Animal
                // 子类: fn get() -> Dog -> 合法！(Dog < Animal)
                // 规则：ParentRet.is_assignable_from(ChildRet)
                // (这一步之前的代码其实写对了，再次确认一下)
                if !expected_ret.is_assignable_from(&child_sig.ret) {
                    let m_name = self.ctx.resolve_symbol(*name).to_string();
                    let c_str = child_sig.ret.display(&self.ctx.interner).to_string();
                    let e_str = expected_ret.display(&self.ctx.interner).to_string();

                    self.report(
                        span,
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

    fn check_method_body(&mut self, method: &MethodDefinition, current_table: &TableInfo) {
        let body_block = match &method.body {
            Some(b) => b,
            None => return, // 抽象方法
        };

        // 保存状态
        let prev_return_type = self.current_return_type.clone();

        let sig = current_table.methods.get(&method.name).unwrap();
        let expected_ret = sig.ret.clone();
        self.current_return_type = Some(expected_ret.clone());

        self.scopes.enter_scope();

        // 1. 定义 `self`
        // 构造当前 Table 的唯一 ID
        // 假设 TableId 在 super 模块或者已经引入
        let table_id = super::TableId(current_table.file_id, current_table.name);

        let self_type = if !current_table.generic_params.is_empty() {
            let args = current_table
                .generic_params
                .iter()
                .map(|s| Type::GenericParam(*s))
                .collect();

            Type::GenericInstance {
                base: table_id, // [Fix] 这里 base 也需要是 TableId
                args,
            }
        } else {
            // [Fix] 正确构造: Type::Table(TableId(file_id, name))
            Type::Table(table_id)
        };

        let _ = self.scopes.define(
            self.ctx.intern("self"),
            self_type,
            SymbolKind::Variable,
            false,
        );

        // 2. 定义参数
        for param in &method.params {
            // 从签名中查找已经 resolve 好的类型
            if let Some((_, p_ty)) = sig.params.iter().find(|(n, _)| *n == param.name) {
                let _ = self
                    .scopes
                    .define(param.name, p_ty.clone(), SymbolKind::Parameter, false);
            }
        }

        // 3. 检查 Body
        let body_type = self.check_block(body_block);

        // 4. 检查返回值
        if !expected_ret.is_assignable_from(&body_type) {
            if expected_ret == Type::Unit {
                // 期望 Unit (Void)，通常允许隐式返回（或者忽略最后表达式的值）
                // 具体行为取决于 Loom 语言规范，这里暂时忽略不报错
            } else {
                // [Fix] 删除之前那些手动转换 String 和构造 SemanticErrorKind 的错误代码
                // 直接调用 Helper，一键完成报错
                self.error_type_mismatch(
                    method.span, // 如果能拿到 body 的 span (block.span) 会更精确，用 method.span 也凑合
                    &expected_ret,
                    &body_type,
                );
            }
        }

        self.scopes.exit_scope();
        self.current_return_type = prev_return_type;
    }

    /// Block 检查 (Helper)
    pub fn check_block(&mut self, block: &Block) -> Type {
        let mut last_type = Type::Unit;
        for stmt in &block.statements {
            last_type = self.check_expression(stmt);
        }
        last_type
    }

    fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        // 1. 基础检查
        if target.is_assignable_from(source) {
            return true;
        }

        // 2. 复杂类型检查
        match (target, source) {
            // Case A: 普通类的继承 (Animal vs Dog)
            (Type::Table(target_id), Type::Table(source_id)) => {
                self.is_subtype(source_id.symbol(), target_id.symbol())
            }

            // Case B: 泛型实例的继承 (List<Animal> vs List<Dog>)
            // [New] 泛型协变检查
            (
                Type::GenericInstance {
                    base: target_base,
                    args: target_args,
                },
                Type::GenericInstance {
                    base: source_base,
                    args: source_args,
                },
            ) => {
                // 2.1 基础类必须相同 (或者存在继承关系，暂时只处理相同的情况)
                // 例如: 不能把 Map<String> 赋值给 List<String>
                if target_base.symbol() != source_base.symbol() {
                    return false;
                }

                // 2.2 参数数量必须一致
                if target_args.len() != source_args.len() {
                    return false;
                }

                // 2.3 [核心] 逐个检查泛型参数的兼容性 (协变)
                // 规则：Source 的参数必须是 Target 参数的子类
                // 即: List<Dog> 可以赋值给 List<Animal>，因为 Dog 是 Animal
                for (t_arg, s_arg) in target_args.iter().zip(source_args.iter()) {
                    // 递归调用 check_type_compatibility
                    if !self.check_type_compatibility(t_arg, s_arg) {
                        return false;
                    }
                }

                true
            }

            // Case C: 数组的协变 (Array<Dog> -> Array<Animal>)
            // 如果你的 Type::Array 是独立的枚举，也要加类似逻辑
            (Type::Array(t_inner), Type::Array(s_inner)) => {
                self.check_type_compatibility(t_inner, s_inner)
            }

            _ => false,
        }
    }

    /// [New] 递归检查 source 是否继承自 target
    fn is_subtype(&self, child_sym: Symbol, target_sym: Symbol) -> bool {
        // 递归基：如果 ID 相等，就是子类型 (Self is subtype of Self)
        if child_sym == target_sym {
            return true;
        }

        // 查找 Child 的定义
        if let Some(info) = self.tables.get(&child_sym) {
            // 看看它有没有父类
            if let Some(parent_type) = &info.parent {
                // 获取父类的 Symbol
                if let Some(parent_sym) = parent_type.get_base_symbol() {
                    // 递归检查：父类是不是目标类型的子类？
                    return self.is_subtype(parent_sym, target_sym);
                }
            }
        }

        // 查不到定义或没有父类，说明继承链断了，匹配失败
        false
    }
}
