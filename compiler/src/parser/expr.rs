mod postfix;
mod prefix;

use crate::ast::*;
use crate::parser::{ParseResult, Parser};
use crate::token::TokenKind;

impl<'a> Parser<'a> {
    /// 核心入口：解析表达式
    pub fn parse_expression(&mut self) -> ParseResult<Expression> {
        self.parse_expression_bp(0)
    }

    /// 获取左结合力 (Left Binding Power)
    fn get_binding_power(&self, kind: TokenKind) -> Option<u8> {
        match kind {
            // --- 赋值 (优先级最低 10) ---
            // Loom 支持 =, += 等赋值作为表达式
            TokenKind::Assign | TokenKind::PlusAssign | TokenKind::MinusAssign |
            TokenKind::StarAssign | TokenKind::SlashAssign | TokenKind::PercentAssign => Some(10),

            // --- 逻辑 (20-30) ---
            TokenKind::Or => Some(20),  // ||
            TokenKind::And => Some(30), // &&

            // --- 比较 (40) ---
            TokenKind::Equal | TokenKind::NotEqual |
            TokenKind::LessThan | TokenKind::LessEqual |
            TokenKind::GreaterThan | TokenKind::GreaterEqual => Some(40),
            // --- 加减 (50) ---
            TokenKind::Plus | TokenKind::Minus => Some(50),

            // --- 乘除模 (60) ---
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some(60),

            // --- 范围 (70) ---
            TokenKind::DotDot => Some(70),

            TokenKind::As => Some(80),

            // --- 后缀 (90) ---
            TokenKind::LeftParen |      // Call
            TokenKind::LeftBracket |    // Index
            TokenKind::Dot              // Member Access
            => Some(90),

            _ => None,
        }
    }

    /// Pratt Parsing Loop
    fn parse_expression_bp(&mut self, min_bp: u8) -> ParseResult<Expression> {
        // 1. Prefix
        let mut lhs = self.parse_prefix()?;

        // 2. Infix / Postfix
        loop {
            let next_token = self.peek();
            if next_token.kind == TokenKind::EOF {
                break;
            }

            // 特殊检查：如果遇到 '<'，先判断它是泛型还是小于号
            if next_token.kind == TokenKind::LessThan {
                if self.looks_like_generic_args() {
                    // 这是一个泛型调用！转交给 postfix 处理
                    lhs = self.parse_postfix_generic(lhs)?;
                    continue; // 继续下一次循环
                }
                // 否则，它是小于号，继续下面的 op_bp 逻辑
            }

            let op_bp = match self.get_binding_power(next_token.kind) {
                Some(bp) if bp >= min_bp => bp,
                _ => break,
            };

            // 分流处理：后缀 / 赋值 / 二元
            if matches!(
                next_token.kind,
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::Dot
            ) {
                lhs = self.parse_postfix(lhs)?;
            }
            // [Fix] 专门处理 Range (..)
            else if next_token.kind == TokenKind::DotDot {
                self.advance(); // 吃掉 ..

                // Range 是左结合还是右结合？通常无所谓，这里用 op_bp
                let rhs = self.parse_expression_bp(op_bp)?;

                let span = lhs.span.to(rhs.span);

                // 生成专门的 Range 节点
                lhs = self.make_node(
                    ExpressionData::Range {
                        start: Box::new(lhs),
                        end: Box::new(rhs),
                        inclusive: false, // Loom 默认为 0..5 (不包含5)
                    },
                    span,
                );
            }
            // [New] 赋值操作处理
            else if let Some(assign_op) = self.map_assign_op(next_token.kind) {
                self.advance(); // eat op
                // 赋值是右结合 (Right Associative) -> 递归调用时 min_bp 保持不变 (或者 op_bp)
                // a = b = c  => a = (b = c)
                let r_bp = op_bp; // 右结合
                let rhs = self.parse_expression_bp(r_bp)?;

                let span = lhs.span.to(rhs.span);
                lhs = self.make_node(
                    ExpressionData::Assign {
                        op: assign_op,
                        target: Box::new(lhs),
                        value: Box::new(rhs),
                    },
                    span,
                );
            }
            // [New] 处理 as 类型转换
            else if next_token.kind == TokenKind::As {
                self.advance(); // 吃掉 'as'

                // 解析右边的类型
                // 注意：这里不是 parse_expression，而是 parse_type
                let target_type = self.parse_type()?;

                let span = lhs.span.to(target_type.span);

                lhs = self.make_node(
                    ExpressionData::Cast {
                        expr: Box::new(lhs),
                        target_type,
                    },
                    span,
                );
            }
            // 二元操作处理
            else {
                self.advance(); // eat op
                let op = self.map_binary_op(next_token.kind);

                // 左结合
                let r_bp = op_bp + 1;
                let rhs = self.parse_expression_bp(r_bp)?;

                let span = lhs.span.to(rhs.span);
                lhs = self.make_node(
                    ExpressionData::Binary {
                        op,
                        left: Box::new(lhs),
                        right: Box::new(rhs),
                    },
                    span,
                );
            }
        }
        Ok(lhs)
    }

    // Helper: Map Token to AssignOp
    fn map_assign_op(&self, kind: TokenKind) -> Option<AssignOp> {
        match kind {
            TokenKind::Assign => Some(AssignOp::Assign),
            TokenKind::PlusAssign => Some(AssignOp::PlusAssign),
            TokenKind::MinusAssign => Some(AssignOp::MinusAssign),
            TokenKind::StarAssign => Some(AssignOp::MulAssign),
            TokenKind::SlashAssign => Some(AssignOp::DivAssign),
            TokenKind::PercentAssign => Some(AssignOp::ModAssign),
            _ => None,
        }
    }

    fn map_binary_op(&self, kind: TokenKind) -> BinaryOp {
        match kind {
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Percent => BinaryOp::Mod,
            TokenKind::Equal => BinaryOp::Eq,
            TokenKind::NotEqual => BinaryOp::Neq,
            TokenKind::LessThan => BinaryOp::Lt,
            TokenKind::GreaterThan => BinaryOp::Gt,
            TokenKind::LessEqual => BinaryOp::Lte,
            TokenKind::GreaterEqual => BinaryOp::Gte,
            TokenKind::And => BinaryOp::And,
            TokenKind::Or => BinaryOp::Or,
            // 赋值算特殊，但在 AST 里可能只是 BinaryOp::Assign 或者单独节点
            // 如果 ExpressionData 没有 Assign，则需要扩展
            _ => BinaryOp::Eq, // Fallback/Panic
        }
    }

    /// 检查当前的 '<' 是否看起来像泛型参数列表
    /// 规则：
    /// 1. 必须以 Type 开始
    /// 2. 必须以 '>' 结束
    /// 3. '>' 后面通常紧跟着 '(' (函数调用)
    fn looks_like_generic_args(&mut self) -> bool {
        // 假设当前 peek() 是 '<'
        let mut depth = 0;
        let mut i = 1; // 0 is '<'

        loop {
            let tok = self.peek_nth(i);
            match tok.kind {
                TokenKind::EOF | TokenKind::Newline => return false, // 泛型不应该跨语句

                // 可能是类型的部分
                TokenKind::Identifier | TokenKind::Comma | TokenKind::SmallSelf => {}

                // 嵌套泛型 List<Vec<T>>
                TokenKind::LessThan => depth += 1,

                TokenKind::GreaterThan => {
                    if depth == 0 {
                        // 找到了匹配的 '>'
                        // 关键判断：在 Loom 中，泛型通常用于函数调用 run<T>()
                        // 所以检查下一个 token 是否是 '('
                        let next = self.peek_nth(i + 1);
                        return next.kind == TokenKind::LeftParen;
                    }
                    depth -= 1;
                }

                // 快速失败：如果出现操作符、字面量等，肯定不是泛型
                // e.g. run<10> (Loom 泛型目前只支持类型，不支持 const generics)
                TokenKind::Equal
                | TokenKind::Plus
                | TokenKind::StringLiteral
                | TokenKind::Integer => return false,

                _ => {} // 其他 token 继续扫描
            }
            i += 1;
        }
    }
}
