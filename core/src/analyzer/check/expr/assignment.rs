use crate::analyzer::{Analyzer, SemanticErrorKind, SymbolKind, Type};
use crate::ast::{AssignOp, BinaryOp, Expression, ExpressionData};
use crate::source::FileId;
use crate::utils::{Span, Symbol};

impl<'a> Analyzer<'a> {
    /// [入口] 赋值表达式检查
    /// 负责分发逻辑：是复合赋值？是变量赋值？还是字段/索引赋值？
    pub fn check_assignment_expr(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        span: Span, // 整个赋值表达式的 Span
    ) -> Type {
        // 1. 先检查右值 (RHS)
        let rhs_ty = self.check_expression(right);
        if rhs_ty == Type::Error {
            return Type::Error;
        }

        // 2. 处理复合赋值 (+=, -=, *=, /=)
        if op != AssignOp::Assign {
            return self.check_compound_assignment(op, left, rhs_ty, span);
        }

        // 3. 处理简单赋值 (=)
        // 根据左值的类型分发给不同的处理函数
        match &left.data {
            ExpressionData::Identifier(sym) => {
                self.check_variable_assignment(*sym, left, rhs_ty, right.span)
            }
            ExpressionData::FieldAccess { target, field } => {
                self.check_field_assignment(target, *field, rhs_ty, right.span)
            }
            ExpressionData::Index { target, index } => {
                self.check_index_assignment(target, index, rhs_ty, right.span)
            }
            _ => {
                self.report(
                    left.span,
                    SemanticErrorKind::InvalidAssignmentTarget("Invalid assignment target".into()),
                );
                Type::Error
            }
        }
    }

    // --- 下面是拆分出来的辅助函数 ---

    /// 处理复合赋值逻辑 (Logic for +=, -= etc.)
    fn check_compound_assignment(
        &mut self,
        op: AssignOp,
        left: &Expression,
        rhs_ty: Type,
        span: Span,
    ) -> Type {
        let lhs_ty = self.check_expression(left);
        if lhs_ty == Type::Error {
            return Type::Error;
        }

        let bin_op = match op {
            AssignOp::PlusAssign => BinaryOp::Add,
            AssignOp::MinusAssign => BinaryOp::Sub,
            AssignOp::MulAssign => BinaryOp::Mul,
            AssignOp::DivAssign => BinaryOp::Div,
            AssignOp::Assign => unreachable!(),
        };

        // 1. 检查运算是否合法 (例如 int += int)
        let result_ty = self.check_binary_op(bin_op, lhs_ty.clone(), rhs_ty, span);
        if result_ty == Type::Error {
            return Type::Error;
        }

        // 2. 检查结果是否能写回左值 (例如 int += float -> float, 但左边是 int，报错)
        if !lhs_ty.is_assignable_from(&result_ty) {
            self.error_type_mismatch(span, &lhs_ty, &result_ty);
        }
        Type::Unit
    }

    /// 处理普通变量赋值 (Logic for `x = val`)
    /// 处理普通变量赋值 (Logic for `x = val`)
    fn check_variable_assignment(
        &mut self,
        sym: Symbol,
        left_expr: &Expression,
        rhs_ty: Type,
        rhs_span: Span,
    ) -> Type {
        // --- 第一阶段：只读查询 & 克隆数据 ---
        // 我们先解析符号，如果有值，就把需要的数据 clone 出来。
        // map 结束后，临时借用的 info 就会被销毁，self 也就自由了。
        let resolved_data = self.scopes.resolve(sym).map(|info| {
            (
                info.ty.clone(),   // 需要 Clone 类型
                info.defined_file, // Copy (FileId 是 usize)
                info.defined_span, // Copy (Span 是两个 usize)
            )
        });

        // --- 第二阶段：可变操作 ---
        if let Some((var_ty, def_file, def_span)) = resolved_data {
            // Case A: 变量已存在 -> 检查类型兼容性
            // 注意：这里传的是 &var_ty (我们 clone 出来的)，而不是 &info.ty
            if !self.check_type_compatibility(&var_ty, &rhs_ty) {
                let var_name = self.ctx.resolve_symbol(sym).to_string();

                // 这里需要用到 ctx，Analyzer 持有 ctx 的 mut 引用，所以这一步也是没问题的
                let var_ty_str = var_ty.display(&self.ctx).to_string();
                let rhs_ty_str = rhs_ty.display(&self.ctx).to_string();

                self.report(
                    rhs_span,
                    SemanticErrorKind::Custom(format!(
                        "Type mismatch: variable '{}' is {}, cannot assign {}",
                        var_name, var_ty_str, rhs_ty_str
                    )),
                );
            }

            // [LSP] 记录变量的写引用 (Usage)
            self.record_def(left_expr.id, def_file, def_span);
            self.record_type(left_expr.id, var_ty);
        } else {
            // Case B: 变量不存在 -> 定义新变量
            let _ = self.scopes.define(
                sym,
                rhs_ty,
                SymbolKind::Variable,
                left_expr.span,
                self.current_file_id,
                false,
            );
        }

        Type::Unit
    }

    /// 处理字段赋值 (Logic for `obj.field = val`)
    /// 包含解决 Borrow Checker 冲突的 LookUpResult 模式
    fn check_field_assignment(
        &mut self,
        target: &Expression,
        field: Symbol,
        rhs_ty: Type,
        rhs_span: Span,
    ) -> Type {
        let target_ty = self.check_expression(target);

        let expected_ty = match target_ty {
            Type::Table(table_id) | Type::GenericInstance { base: table_id, .. } => {
                let lookup_result = if let Some(info) = self.find_table_info(table_id) {
                    if let Some(field_info) = info.fields.get(&field) {
                        LookupResult::Found {
                            file_id: info.file_id,
                            span: field_info.span,
                            ty: field_info.ty.clone(), // Clone 类型以断开引用
                        }
                    } else {
                        LookupResult::FieldMissing
                    }
                } else {
                    LookupResult::TableMissing
                };

                // --- Phase 2: 写入操作 (Mutable Borrow) ---
                // 此时 self 借用已释放，可以安全调用 record_def 或 report
                match lookup_result {
                    LookupResult::Found { file_id, span, ty } => {
                        // [LSP] 记录定义跳转
                        // 当用户按住 Ctrl 点击 `target` (如 `user`) 时，通常由 check_expression 处理了
                        // 这里我们记录 `target` 指向了字段定义？
                        // *纠正*：理想情况下这里应该记录 `field` 的位置，但 AST 里 `field` 只是 Symbol。
                        // 这里我们暂时把 target 的引用记录再次指向字段定义，或者不做操作。
                        // 原代码逻辑：self.record_def(target.id, ...)
                        self.record_def(target.id, file_id, span);
                        self.record_type(target.id, ty.clone());
                        ty
                    }
                    LookupResult::FieldMissing => {
                        let f_name = self.ctx.resolve_symbol(field).to_string();
                        self.report(target.span, SemanticErrorKind::UndefinedSymbol(f_name));
                        Type::Error
                    }
                    LookupResult::TableMissing => Type::Error,
                }
            }
            Type::Error => Type::Error, // 级联错误，忽略
            _ => {
                self.report(
                    target.span,
                    SemanticErrorKind::InvalidAssignmentTarget(
                        "Cannot assign fields on non-table type".into(),
                    ),
                );
                Type::Error
            }
        };

        // 检查类型兼容性
        if expected_ty != Type::Error && !self.check_type_compatibility(&expected_ty, &rhs_ty) {
            self.error_type_mismatch(rhs_span, &expected_ty, &rhs_ty);
        }

        Type::Unit
    }

    /// 处理索引赋值 (Logic for `arr[i] = val`)
    fn check_index_assignment(
        &mut self,
        target: &Expression,
        index: &Expression,
        rhs_ty: Type,
        rhs_span: Span,
    ) -> Type {
        let target_ty = self.check_expression(target);
        let index_ty = self.check_expression(index);

        match target_ty {
            Type::Array(inner_ty) => {
                // 1. 索引必须是整数
                if index_ty != Type::Int {
                    self.report(
                        index.span,
                        SemanticErrorKind::InvalidIndexType("Array index must be integer".into()),
                    );
                }
                // 2. 值必须与数组元素类型兼容
                if !self.check_type_compatibility(&inner_ty, &rhs_ty) {
                    self.error_type_mismatch(rhs_span, &inner_ty, &rhs_ty);
                }
            }
            Type::Error => {}
            _ => {
                self.report(
                    target.span,
                    SemanticErrorKind::TypeNotIndexable(
                        "Index assignment only supported for Arrays".into(),
                    ),
                );
            }
        }
        Type::Unit
    }
}

enum LookupResult {
    Found {
        file_id: FileId,
        span: Span,
        ty: Type,
    },
    FieldMissing,
    TableMissing,
}
