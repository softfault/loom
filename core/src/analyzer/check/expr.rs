use super::*;
use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::{Span, TableId};

impl<'a> Analyzer<'a> {
    // === 1. 字面量与原子 ===

    pub(super) fn check_literal_expr(&mut self, lit: &Literal) -> Type {
        match lit {
            Literal::Int(_) => Type::Int,
            Literal::Float(_) => Type::Float,
            Literal::Bool(_) => Type::Bool,
            Literal::String(_) => Type::Str,
            Literal::Char(_) => Type::Char,
            Literal::Nil => Type::Nil,
        }
    }

    pub(super) fn check_identifier_expr(&mut self, sym: Symbol, span: crate::utils::Span) -> Type {
        if let Some(info) = self.scopes.resolve(sym) {
            info.ty.clone()
        } else {
            let name = self.ctx.resolve_symbol(sym).to_string();
            // 现在有了 Span，可以精准报错了
            self.report(span, SemanticErrorKind::UndefinedSymbol(name));
            Type::Error
        }
    }

    // === 2. 复合数据结构 (Array, Tuple) ===

    pub(super) fn check_array_expr(&mut self, elements: &[Expression]) -> Type {
        if elements.is_empty() {
            return Type::Array(Box::new(Type::Infer));
        }

        let first_ty = self.check_expression(&elements[0]);

        for (i, expr) in elements.iter().enumerate().skip(1) {
            let ty = self.check_expression(expr);
            if !first_ty.is_assignable_from(&ty) {
                let expected_str = first_ty.display(&self.ctx.interner).to_string();
                let found_str = ty.display(&self.ctx.interner).to_string();

                self.report(
                    expr.span,
                    SemanticErrorKind::ArrayElementTypeMismatch {
                        index: i,
                        expected: expected_str,
                        found: found_str,
                    },
                );
                return Type::Error;
            }
        }

        Type::Array(Box::new(first_ty))
    }

    pub(super) fn check_tuple_expr(&mut self, elements: &[Expression]) -> Type {
        let types = elements.iter().map(|e| self.check_expression(e)).collect();
        Type::Tuple(types)
    }

    // === 3. 运算 (Unary, Binary) ===

    pub(super) fn check_unary_expr(&mut self, op: UnaryOp, operand: &Expression) -> Type {
        let ty = self.check_expression(operand);
        if ty == Type::Error {
            return Type::Error;
        }

        match op {
            UnaryOp::Neg => {
                if ty == Type::Int || ty == Type::Float {
                    ty
                } else {
                    let ty_str = ty.display(&self.ctx.interner).to_string();
                    self.report(
                        operand.span,
                        SemanticErrorKind::InvalidUnaryOperand {
                            op: "-".to_string(),
                            ty: ty_str,
                        },
                    );
                    Type::Error
                }
            }
            UnaryOp::Not => {
                if ty == Type::Bool {
                    Type::Bool
                } else {
                    let ty_str = ty.display(&self.ctx.interner).to_string();
                    self.report(
                        operand.span,
                        SemanticErrorKind::InvalidUnaryOperand {
                            op: "!".to_string(),
                            ty: ty_str,
                        },
                    );
                    Type::Error
                }
            }
        }
    }

    pub(super) fn check_binary_expr(
        &mut self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
        span: Span, // Binary Expr 应该传入 Span
    ) -> Type {
        let l_ty = self.check_expression(left);
        let r_ty = self.check_expression(right);

        // 调用之前的 check_binary_op，传入 span
        self.check_binary_op(op, l_ty, r_ty, span)
    }

    // 需修改 check_binary_op 签名以接收 Span
    fn check_binary_op(&mut self, op: BinaryOp, left: Type, right: Type, span: Span) -> Type {
        if left == Type::Error || right == Type::Error {
            return Type::Error;
        }

        match op {
            // === 算术运算 ===
            BinaryOp::Add => {
                // 1. 特殊处理：字符串拼接 (Str + Any)
                if left == Type::Str {
                    // Loom 特性：允许 "Text " + 123 -> "Text 123"
                    // 这里不需要检查 right 的类型，因为任何类型理论上都能 to_string
                    return Type::Str;
                }
                if right == Type::Str {
                    // 也支持 123 + " Text" -> "123 Text"
                    return Type::Str;
                }

                // 2. 常规数字相加
                if left == Type::Int && right == Type::Int {
                    return Type::Int;
                }
                if left == Type::Float && right == Type::Float {
                    return Type::Float;
                }

                // 错误处理
                let l_str = left.display(&self.ctx.interner).to_string();
                let r_str = right.display(&self.ctx.interner).to_string();
                self.report(
                    span,
                    SemanticErrorKind::InvalidBinaryOperand {
                        op: "+".to_string(),
                        lhs: l_str,
                        rhs: r_str,
                    },
                );
                Type::Error
            }

            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if left == Type::Int && right == Type::Int {
                    return Type::Int;
                }
                if left == Type::Float && right == Type::Float {
                    return Type::Float;
                }

                let l_str = left.display(&self.ctx.interner).to_string();
                let r_str = right.display(&self.ctx.interner).to_string();
                self.report(
                    span,
                    SemanticErrorKind::InvalidBinaryOperand {
                        op: format!("{:?}", op),
                        lhs: l_str,
                        rhs: r_str,
                    },
                );
                Type::Error
            }

            // === 比较运算 ===
            BinaryOp::Eq | BinaryOp::Neq => {
                // 允许任何类型比较，只要类型相同，或者一边是 Any/Nil
                // 这里暂定返回 Bool，不做严格类型检查，交给 Runtime 处理是否相等
                Type::Bool
            }

            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Lte | BinaryOp::Gte => {
                if left.is_numeric() && right.is_numeric() {
                    Type::Bool
                } else {
                    let l_str = left.display(&self.ctx.interner).to_string();
                    let r_str = right.display(&self.ctx.interner).to_string();
                    self.report(
                        span,
                        SemanticErrorKind::InvalidBinaryOperand {
                            op: format!("{:?}", op),
                            lhs: l_str,
                            rhs: r_str,
                        },
                    );
                    Type::Error
                }
            }

            // === [Fix] 逻辑运算 ===
            BinaryOp::And | BinaryOp::Or => {
                if left == Type::Bool && right == Type::Bool {
                    Type::Bool
                } else {
                    let l_str = left.display(&self.ctx.interner).to_string();
                    let r_str = right.display(&self.ctx.interner).to_string();
                    self.report(
                        span,
                        SemanticErrorKind::InvalidBinaryOperand {
                            op: if op == BinaryOp::And {
                                "and".into()
                            } else {
                                "or".into()
                            },
                            lhs: l_str,
                            rhs: r_str,
                        },
                    );
                    Type::Error
                }
            }

            // 其他未处理的操作符
            _ => {
                self.report(
                    span,
                    SemanticErrorKind::Custom(format!("Operator {:?} not supported yet", op)),
                );
                Type::Error
            }
        }
    }

    pub(super) fn check_assignment_expr(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        span: Span, // Assign Expr Span
    ) -> Type {
        let rhs_ty = self.check_expression(right);
        if rhs_ty == Type::Error {
            return Type::Error;
        }

        // 处理复合赋值
        if op != AssignOp::Assign {
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

            // 检查运算是否合法
            let result_ty = self.check_binary_op(bin_op, lhs_ty.clone(), rhs_ty, span);
            if result_ty == Type::Error {
                return Type::Error;
            }

            // 检查赋值回写
            // 例如: var a: int = 1; a += "hello";
            // result_ty 变成了 String (假设 add 支持)，但 lhs_ty 是 Int
            if !lhs_ty.is_assignable_from(&result_ty) {
                // [Fix] 直接使用 helper，移除多余的手动 report
                // helper 内部会自动调用 display() 把 Type 转为 String 并生成 TypeMismatch 错误
                self.error_type_mismatch(span, &lhs_ty, &result_ty);
            }
            return Type::Unit;
        }

        // 处理简单赋值
        match &left.data {
            ExpressionData::Identifier(sym) => {
                if let Some(info) = self.scopes.resolve(*sym) {
                    // [修改点 1] 变量赋值
                    // 旧代码: if !info.ty.is_assignable_from(&rhs_ty) {
                    // 新代码: 使用 check_type_compatibility 支持继承/协变
                    if !self.check_type_compatibility(&info.ty, &rhs_ty) {
                        let var_name = self.ctx.resolve_symbol(*sym).to_string();
                        let var_ty_str = info.ty.display(&self.ctx.interner).to_string();
                        let rhs_ty_str = rhs_ty.display(&self.ctx.interner).to_string();

                        self.report(
                            right.span,
                            SemanticErrorKind::Custom(format!(
                                "Type mismatch: variable '{}' is {}, cannot assign {}",
                                var_name, var_ty_str, rhs_ty_str
                            )),
                        );
                    }
                } else {
                    // 新定义 (弱类型推导，或者首次赋值)
                    let _ = self
                        .scopes
                        .define(*sym, rhs_ty, SymbolKind::Variable, false);
                }
                Type::Unit
            }

            ExpressionData::FieldAccess { target, field } => {
                let target_ty = self.check_expression(target);
                let expected_ty = match target_ty {
                    Type::Table(t_sym) | Type::GenericInstance { base: t_sym, .. } => {
                        if let Some(info) = self.tables.get(&t_sym.symbol()) {
                            if let Some(field_ty) = info.fields.get(field) {
                                field_ty.clone()
                            } else {
                                let f_name = self.ctx.resolve_symbol(*field).to_string();
                                self.report(left.span, SemanticErrorKind::UndefinedSymbol(f_name));
                                return Type::Error;
                            }
                        } else {
                            Type::Error
                        }
                    }
                    Type::Error => Type::Error,
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

                if expected_ty != Type::Error
                    && !self.check_type_compatibility(&expected_ty, &rhs_ty)
                {
                    self.error_type_mismatch(right.span, &expected_ty, &rhs_ty);
                }
                Type::Unit
            }

            ExpressionData::Index { target, index } => {
                let target_ty = self.check_expression(target);
                let index_ty = self.check_expression(index);

                match target_ty {
                    Type::Array(inner_ty) => {
                        if index_ty != Type::Int {
                            self.report(
                                index.span,
                                SemanticErrorKind::InvalidIndexType(
                                    "Array index must be integer".into(),
                                ),
                            );
                        }
                        if !self.check_type_compatibility(&inner_ty, &rhs_ty) {
                            self.error_type_mismatch(right.span, &inner_ty, &rhs_ty);
                        }
                    }
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

            _ => {
                self.report(
                    left.span,
                    SemanticErrorKind::InvalidAssignmentTarget("Invalid assignment target".into()),
                );
                Type::Error
            }
        }
    }

    pub(super) fn check_range_expr(
        &mut self,
        start: &Expression,
        end: &Expression,
        span: Span,
    ) -> Type {
        let start_ty = self.check_expression(start);
        let end_ty = self.check_expression(end);

        if start_ty == Type::Int && end_ty == Type::Int {
            Type::Range(Box::new(Type::Int))
        } else {
            self.report(
                span,
                SemanticErrorKind::Custom("Range bounds must be integers".into()),
            );
            Type::Error
        }
    }

    /// 核心入口：检查表达式
    /// 这是一个干净的 Dispatcher，负责将 Expression 节点分发给具体的检查逻辑
    pub fn check_expression(&mut self, expr: &Expression) -> Type {
        match &expr.data {
            // 字面量
            ExpressionData::Literal(lit) => self.check_literal_expr(lit),

            // [Fix] 这里的 Identifier 现在传入 span，以便在变量未定义时准确报错
            ExpressionData::Identifier(sym) => self.check_identifier_expr(*sym, expr.span),

            // 复合结构
            ExpressionData::Array(elements) => self.check_array_expr(elements),
            ExpressionData::Tuple(elements) => self.check_tuple_expr(elements),

            // 运算 (传入 span)
            ExpressionData::Unary { op, expr: operand } => self.check_unary_expr(*op, operand),
            ExpressionData::Binary { op, left, right } => {
                self.check_binary_expr(*op, left, right, expr.span)
            }
            ExpressionData::Range { start, end, .. } => {
                self.check_range_expr(start, end, expr.span)
            }

            // 访问与调用 (传入 span)
            ExpressionData::FieldAccess { target, field } => {
                // check_access_expr 内部会处理 target 的检查，但若 access 失败需要 span
                self.check_access_expr(target, *field)
            }
            ExpressionData::Index { target, index } => self.check_index_expr(target, index),
            ExpressionData::Call {
                callee,
                generic_args,
                args,
            } => self.check_call_expr(callee, generic_args, args),

            // 控制流 (传入 span)
            ExpressionData::Block(block) => self.check_block_expr(block),
            ExpressionData::If {
                condition,
                then_block,
                else_block,
            } => self.check_if_expr(condition, then_block, else_block, expr.span),

            ExpressionData::While { condition, body } => self.check_while_expr(condition, body),
            ExpressionData::For {
                iterator,
                iterable,
                body,
            } => self.check_for_expr(*iterator, iterable, body),

            // Return / Break / Continue
            ExpressionData::Return(val) => self.check_return_expr(val, expr.span),
            ExpressionData::Break { .. } => {
                // 可以在这里检查是否在循环内，如果不在则报错
                // 简单起见返回 Never
                Type::Never
            }
            ExpressionData::Continue => Type::Never,

            // 赋值 (传入 span)
            ExpressionData::Assign { op, target, value } => {
                self.check_assignment_expr(*op, target, value, expr.span)
            }

            // 变量定义 (let a: int = 1)
            ExpressionData::VariableDefinition { name, ty, init, .. } => {
                let init_ty = self.check_expression(init);

                let final_ty = if let Some(t_ref) = ty {
                    // 解析显式类型标注
                    let decl_ty = self.resolve_ast_type(t_ref, &std::collections::HashSet::new());

                    // 检查初始值类型
                    if !self.check_type_compatibility(&decl_ty, &init_ty) {
                        // 使用刚才定义的 helper
                        self.error_type_mismatch(init.span, &decl_ty, &init_ty);
                    }
                    decl_ty
                } else {
                    // 推导类型
                    init_ty
                };

                // 定义变量 (allow_shadow = true)
                // span 用于定义冲突时的报错（虽然这里 allow_shadow=true 不会报冲突）
                let _ = self
                    .scopes
                    .define(*name, final_ty, SymbolKind::Variable, true);

                Type::Unit
            }
        }
    }
}
