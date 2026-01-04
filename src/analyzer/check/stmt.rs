use super::*;
use crate::utils::Span;

impl<'a> Analyzer<'a> {
    pub(super) fn check_if_expr(
        &mut self,
        condition: &Expression,
        then_block: &Block,
        else_block: &Option<Block>,
        span: Span,
    ) -> Type {
        let cond_ty = self.check_expression(condition);
        if cond_ty != Type::Bool && cond_ty != Type::Error {
            self.report(
                condition.span,
                SemanticErrorKind::ConditionNotBool("If".into()),
            );
        }

        let then_ty = self.check_block_expr(then_block);

        if let Some(else_blk) = else_block {
            let else_ty = self.check_block_expr(else_blk);

            if then_ty == Type::Never {
                return else_ty;
            }
            if else_ty == Type::Never {
                return then_ty;
            }

            if !then_ty.is_assignable_from(&else_ty) && !else_ty.is_assignable_from(&then_ty) {
                let t_str = then_ty.display(&self.ctx.interner).to_string();
                let e_str = else_ty.display(&self.ctx.interner).to_string();
                self.report(
                    span, // If expression span
                    SemanticErrorKind::IfBranchIncompatible {
                        then_ty: t_str,
                        else_ty: e_str,
                    },
                );
                return Type::Error;
            }
            then_ty
        } else {
            if then_ty != Type::Unit && then_ty != Type::Never && then_ty != Type::Error {
                let t_str = then_ty.display(&self.ctx.interner).to_string();
                self.report(
                    then_block.span, // 最好是指向 then block 结束位置或者整个 if
                    SemanticErrorKind::IfMissingElse(t_str),
                );
                return Type::Error;
            }
            Type::Unit
        }
    }

    pub(super) fn check_while_expr(&mut self, condition: &Expression, body: &Block) -> Type {
        let cond_ty = self.check_expression(condition);
        if cond_ty != Type::Bool && cond_ty != Type::Error {
            self.report(
                condition.span,
                SemanticErrorKind::ConditionNotBool("While".into()),
            );
        }
        self.check_block_expr(body);
        Type::Unit
    }

    pub(super) fn check_for_expr(
        &mut self,
        iterator: Symbol,
        iterable: &Expression,
        body: &Block,
    ) -> Type {
        let iterable_ty = self.check_expression(iterable);

        let item_ty = match iterable_ty {
            Type::Array(inner) => *inner,
            Type::Range(inner) => *inner,
            Type::Str => Type::Str,
            Type::Error => Type::Error,
            _ => {
                let ty_str = iterable_ty.display(&self.ctx.interner).to_string();
                self.report(iterable.span, SemanticErrorKind::TypeNotIterable(ty_str));
                Type::Error
            }
        };

        self.scopes.enter_scope();
        let _ = self
            .scopes
            .define(iterator, item_ty, SymbolKind::Variable, true);
        self.check_block_expr(body);
        self.scopes.exit_scope();
        Type::Unit
    }

    pub(super) fn check_return_expr(
        &mut self,
        val_opt: &Option<Box<Expression>>,
        span: Span,
    ) -> Type {
        // 1. 先检查返回值的表达式（这需要 &mut self，所以要在获取 expected 之前或者之后做，不能夹在中间）
        let actual_type = if let Some(val) = val_opt {
            self.check_expression(val)
        } else {
            Type::Unit
        };

        // 2. [关键修复] Clone 出期望类型，断开与 self 的借用关系
        // 这样 self 就不再被借用了
        let expected_opt = self.current_return_type.clone();

        match expected_opt {
            Some(expected) => {
                // 这里的 expected 是一个独立的 Type 对象，不再指向 self
                if !expected.is_assignable_from(&actual_type) {
                    // 现在可以安全地以 &mut self 调用报错函数了
                    self.error_type_mismatch(span, &expected, &actual_type);
                }
            }
            None => {
                self.report(span, SemanticErrorKind::ReturnOutsideFunction);
            }
        }

        Type::Never
    }

    pub(super) fn check_block_expr(&mut self, block: &Block) -> Type {
        // Block 也是表达式，需要开启新的作用域
        self.scopes.enter_scope();

        let mut last_type = Type::Unit;
        for stmt in &block.statements {
            last_type = self.check_expression(stmt);
        }

        self.scopes.exit_scope();
        last_type
    }

    /// Block 检查 (Helper)
    pub fn check_block(&mut self, block: &Block) -> Type {
        let mut last_type = Type::Unit;
        for stmt in &block.statements {
            last_type = self.check_expression(stmt);
        }
        last_type
    }
}
