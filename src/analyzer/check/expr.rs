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

    // === 4. 访问与调用 ===

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

    // === 5. 控制流 ===

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

    /// 通用辅助函数：报告类型不匹配错误
    pub fn error_type_mismatch(&mut self, span: crate::utils::Span, expected: &Type, found: &Type) {
        // 如果其中一个是 Error 类型，通常意味着之前已经报过错了，为了防止报错刷屏，这里选择静默
        if *expected == Type::Error || *found == Type::Error {
            return;
        }

        let expected_str = expected.display(&self.ctx.interner).to_string();
        let found_str = found.display(&self.ctx.interner).to_string();

        self.report(
            span,
            SemanticErrorKind::TypeMismatch {
                expected: expected_str,
                found: found_str,
            },
        );
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
            return field_ty.substitute(&type_mapping);
        }

        // B. 查找方法 (Method)
        if let Some(method_sig) = table_info.methods.get(&field) {
            // [Key Step] 方法签名也要替换
            // 原始签名: (item: T) -> bool
            // 替换后:   (item: int) -> bool

            let new_params = method_sig
                .params
                .iter()
                .map(|(_, p_ty)| p_ty.substitute(&type_mapping))
                .collect();

            let new_ret = method_sig.ret.substitute(&type_mapping);

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
