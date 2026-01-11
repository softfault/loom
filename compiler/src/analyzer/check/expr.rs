mod assignment;

use std::collections::HashSet;

use super::*;
use crate::analyzer::Span;
use crate::analyzer::errors::SemanticErrorKind;

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

            // 使用 check_type_compatibility 以支持继承关系的数组元素
            // 例如: [Animal, Dog] -> Animal 兼容 Dog
            if !self.check_type_compatibility(&first_ty, &ty) {
                let expected_str = first_ty.display(self.ctx).to_string();
                let found_str = ty.display(self.ctx).to_string();

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
                    let ty_str = ty.display(self.ctx).to_string();
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
                    let ty_str = ty.display(self.ctx).to_string();
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
                let l_str = left.display(self.ctx).to_string();
                let r_str = right.display(self.ctx).to_string();
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

            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if op == BinaryOp::Mod {
                    if left == Type::Int && right == Type::Int {
                        return Type::Int;
                    }
                    // 友好的报错
                    let l_str = left.display(self.ctx).to_string();
                    let r_str = right.display(self.ctx).to_string();
                    self.report(
                        span,
                        SemanticErrorKind::InvalidBinaryOperand {
                            op: "%".to_string(),
                            lhs: l_str,
                            rhs: r_str,
                        },
                    );
                    return Type::Error;
                }

                if left == Type::Int && right == Type::Int {
                    return Type::Int;
                }
                if left == Type::Float && right == Type::Float {
                    return Type::Float;
                }

                let l_str = left.display(self.ctx).to_string();
                let r_str = right.display(self.ctx).to_string();
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
                    let l_str = left.display(self.ctx).to_string();
                    let r_str = right.display(self.ctx).to_string();
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
                    let l_str = left.display(self.ctx).to_string();
                    let r_str = right.display(self.ctx).to_string();
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
        }
    }

    // [New] 核心转换逻辑
    fn check_cast_expr(
        &mut self,
        expr: &Expression,
        target_type_ref: &TypeRef,
        span: Span,
    ) -> Type {
        let source_ty = self.check_expression(expr);
        let target_ty = self.resolve_ast_type(target_type_ref, &HashSet::new());

        if source_ty == Type::Error || target_ty == Type::Error {
            return Type::Error;
        }

        match (&source_ty, &target_ty) {
            // 1. 同类型转换 (No-op)
            (s, t) if s == t => target_ty,

            // 2. 数值转换
            (Type::Int, Type::Float) => target_ty,
            (Type::Float, Type::Int) => target_ty,

            // 3. 继承体系转换 (Upcast/Downcast)
            (Type::Table(s_id), Type::Table(t_id)) => {
                if self.is_subtype(*s_id, *t_id) || self.is_subtype(*t_id, *s_id) {
                    return target_ty;
                }
                self.report_cast_error(span, &source_ty, &target_ty);
                Type::Error
            }

            // 4. [Core Logic] 泛型实例转换 (List<Dog> as List<Animal>)
            (
                Type::GenericInstance {
                    base: s_base,
                    args: s_args,
                },
                Type::GenericInstance {
                    base: t_base,
                    args: t_args,
                },
            ) => {
                // A. 基础类必须相同 (不能把 List<T> 转成 Map<T>)
                if s_base != t_base {
                    self.report_cast_error(span, &source_ty, &target_ty);
                    return Type::Error;
                }

                // B. 参数数量必须一致
                if s_args.len() != t_args.len() {
                    // 这个通常在类型解析阶段就会报错，但防守一下
                    self.report_cast_error(span, &source_ty, &target_ty);
                    return Type::Error;
                }

                // C. 递归检查每一个参数是否可以 Cast
                // 规则：对于泛型参数，我们允许协变 (Covariance) 和逆变 (Contravariance) 的 Cast
                // 也就是只要 T1 可以 Cast 到 T2，那么 List<T1> 就可以 Cast 到 List<T2>
                for (s_arg, t_arg) in s_args.iter().zip(t_args.iter()) {
                    if !self.is_castable(s_arg, t_arg) {
                        self.report_cast_error(span, &source_ty, &target_ty);
                        return Type::Error;
                    }
                }

                target_ty
            }

            // 5. 数组转换 ([Dog] as [Animal])
            // 既然泛型都支持了，数组也应该支持
            (Type::Array(s_inner), Type::Array(t_inner)) => {
                if self.is_castable(s_inner, t_inner) {
                    target_ty
                } else {
                    self.report_cast_error(span, &source_ty, &target_ty);
                    Type::Error
                }
            }

            _ => {
                self.report_cast_error(span, &source_ty, &target_ty);
                Type::Error
            }
        }
    }

    /// [New] 辅助判断是否允许转换 (Check Only)
    /// 这其实是 check_cast_expr 的逻辑复用版，但不报错，只返回 bool
    fn is_castable(&mut self, src: &Type, target: &Type) -> bool {
        if src == target {
            return true;
        }

        match (src, target) {
            (Type::Int, Type::Float) | (Type::Float, Type::Int) => true,

            (Type::Table(s_id), Type::Table(t_id)) => {
                self.is_subtype(*s_id, *t_id) || self.is_subtype(*t_id, *s_id)
            }

            (
                Type::GenericInstance {
                    base: s_base,
                    args: s_args,
                },
                Type::GenericInstance {
                    base: t_base,
                    args: t_args,
                },
            ) => {
                if s_base != t_base || s_args.len() != t_args.len() {
                    return false;
                }
                for (s_arg, t_arg) in s_args.iter().zip(t_args.iter()) {
                    if !self.is_castable(s_arg, t_arg) {
                        return false;
                    }
                }
                true
            }

            (Type::Array(s_inner), Type::Array(t_inner)) => self.is_castable(s_inner, t_inner),

            _ => false,
        }
    }

    fn report_cast_error(&mut self, span: Span, src: &Type, target: &Type) {
        let s_str = src.display(self.ctx).to_string();
        let t_str = target.display(self.ctx).to_string();

        self.report(
            span,
            SemanticErrorKind::InvalidCast {
                // [Fix] 使用专用错误类型
                src: s_str,
                target: t_str,
            },
        );
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

            ExpressionData::Cast { expr, target_type } => {
                self.check_cast_expr(expr, target_type, expr.span)
            }

            // 变量定义 (let a: int = 1)
            ExpressionData::VariableDefinition { name, ty, init, .. } => {
                let init_ty = self.check_expression(init);

                let final_ty = if let Some(t_ref) = ty {
                    // 解析显式类型标注
                    let decl_ty = self.resolve_ast_type(t_ref, &HashSet::new()); // 假设不需要 HashSet 了，或者传个空的

                    // 检查初始值类型
                    if !self.check_type_compatibility(&decl_ty, &init_ty) {
                        self.error_type_mismatch(init.span, &decl_ty, &init_ty);
                    }
                    decl_ty
                } else {
                    // 推导类型
                    init_ty
                };

                // 定义变量
                let _ = self.scopes.define(
                    *name,
                    final_ty,
                    SymbolKind::Variable,
                    expr.span,            // <--- 1. 使用整个表达式的 Span
                    self.current_file_id, // <--- 2. 当前文件 ID
                    true,                 // allow_shadow (Loom 允许遮蔽)
                );

                Type::Unit
            }
        }
    }
}
