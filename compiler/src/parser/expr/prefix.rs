// prefix.rs
use crate::ast::*;
use crate::parser::{ParseError, ParseResult, Parser};
use crate::token::TokenKind;

impl<'a> Parser<'a> {
    pub fn parse_prefix(&mut self) -> ParseResult<Expression> {
        let token = self.peek();

        match token.kind {
            // === 字面量 ===
            TokenKind::Integer => self.parse_int_literal(),
            TokenKind::Float => self.parse_float_literal(),
            TokenKind::StringLiteral => self.parse_string_literal(),
            TokenKind::True => {
                self.advance();
                Ok(self.make_node(ExpressionData::Literal(Literal::Bool(true)), token.span))
            }
            TokenKind::False => {
                self.advance();
                Ok(self.make_node(ExpressionData::Literal(Literal::Bool(false)), token.span))
            }
            TokenKind::Nil => {
                self.advance();
                Ok(self.make_node(ExpressionData::Literal(Literal::Nil), token.span))
            }

            // === 标识符 ===
            TokenKind::Identifier | TokenKind::SmallSelf => {
                self.advance();
                let sym = self.intern_token(token);
                Ok(self.make_node(ExpressionData::Identifier(sym), token.span))
            }

            // === 前缀运算 ===
            TokenKind::Minus | TokenKind::Bang => self.parse_unary(),

            // === 分组/元组 ===
            TokenKind::LeftParen => self.parse_group_or_tuple(), // 确保你有这个函数，或者用简单的分组解析

            // === 数组 ===
            TokenKind::LeftBracket => self.parse_array_literal(),

            // === 控制流 (漏掉的部分补回来！) ===
            TokenKind::If => self.parse_if(),

            TokenKind::Indent => {
                let block = self.parse_block()?; // 返回 Node<BlockData>
                let span = block.span;
                // 包裹进 ExpressionData::Block
                Ok(self.make_node(ExpressionData::Block(block), span))
            }

            // --- 修复点：添加 Return, Loop, Break, Continue, Defer ---
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::For => self.parse_for(),
            TokenKind::While => self.parse_while(),
            _ => Err(ParseError {
                expected: "Expression".into(),
                found: token.kind,
                span: token.span,
                message: format!("Unexpected token at start of expression: {:?}", token.kind),
            }),
        }
    }

    fn parse_unary(&mut self) -> ParseResult<Expression> {
        let op_token = self.advance();
        let op = match op_token.kind {
            TokenKind::Minus => UnaryOp::Neg,
            TokenKind::Bang => UnaryOp::Not,
            _ => unreachable!(),
        };

        let operand = self.parse_expression_bp(80)?; // 高优先级
        let span = op_token.span.to(operand.span);

        Ok(self.make_node(
            ExpressionData::Unary {
                op,
                expr: Box::new(operand),
            },
            span,
        ))
    }

    /// 解析 IF 表达式 (Loom Style)
    /// if cond \n block else \n block
    /// if cond then expr else expr (inline style - 可选)
    fn parse_if(&mut self) -> ParseResult<Expression> {
        let start_span = self.expect(TokenKind::If)?.span;

        let condition = self.parse_expression()?; // 不需要括号

        // 允许 if cond \n Indent ...
        if self.check(TokenKind::Newline) {
            self.advance();
        }

        let then_block = self.parse_block()?;

        // Else
        let else_block = if self.match_token(&[TokenKind::Else]) {
            // else if ...
            if self.check(TokenKind::If) {
                // 递归解析，然后把 if 表达式包在一个 Block 里，或者修改 AST 允许 Else 存 Expr
                // 为了简单，我们把 else-if 变成一个只包含 If 表达式的 Block
                let if_expr = self.parse_if()?;
                Some(self.make_node(
                    BlockData {
                        statements: vec![if_expr],
                    },
                    self.previous_span(),
                ))
            } else {
                // else \n Indent ...
                if self.check(TokenKind::Newline) {
                    self.advance();
                }
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(self.make_node(
            ExpressionData::If {
                condition: Box::new(condition),
                then_block,
                else_block,
            },
            start_span.to(self.previous_span()),
        ))
    }

    // Literal 解析辅助 (复用你的逻辑)
    fn parse_int_literal(&mut self) -> ParseResult<Expression> {
        let token = self.advance();
        let text = self.text(token);
        // 调用你的 parse_int 工具
        let val = parse_int(text).unwrap_or(0) as i64; // 这里简化处理错误
        Ok(self.make_node(ExpressionData::Literal(Literal::Int(val)), token.span))
    }

    fn parse_string_literal(&mut self) -> ParseResult<Expression> {
        let token = self.advance();
        let text = self.text(token);
        // 去掉引号
        let content = &text[1..text.len() - 1];
        let val = self.unescape_string(content);
        Ok(self.make_node(ExpressionData::Literal(Literal::String(val)), token.span))
    }

    fn parse_float_literal(&mut self) -> ParseResult<Expression> {
        let token = self.advance();
        // 移除数字中的下划线 (e.g. 1_000.00)
        let text = self.text(token).replace("_", "");

        match text.parse::<f64>() {
            Ok(val) => {
                // 检查是否溢出为无穷大 (Infinity)
                if val.is_infinite() {
                    self.errors.push(ParseError {
                        expected: "finite float".into(),
                        found: token.kind,
                        span: token.span,
                        message: format!("Float literal '{}' overflows to Infinity", text),
                    });
                    // 错误恢复
                    Ok(self.make_node(ExpressionData::Literal(Literal::Float(0.0)), token.span))
                } else {
                    Ok(self.make_node(ExpressionData::Literal(Literal::Float(val)), token.span))
                }
            }
            Err(_) => {
                // 通常 Lexer 已经保证了格式正确，但为了保险起见
                self.errors.push(ParseError {
                    expected: "valid float".into(),
                    found: token.kind,
                    span: token.span,
                    message: format!("Invalid float literal '{}'", text),
                });

                Ok(self.make_node(ExpressionData::Literal(Literal::Float(0.0)), token.span))
            }
        }
    }

    /// 解析数组字面量
    /// 语法: [expr, expr, ...] (支持尾后逗号)
    fn parse_array_literal(&mut self) -> ParseResult<Expression> {
        let start_span = self.expect(TokenKind::LeftBracket)?.span;
        let mut elements = Vec::new();

        // 循环直到遇到 ']' 或 EOF
        while !self.check(TokenKind::RightBracket) && !self.is_at_end() {
            // 允许数组里换行 (TOML 风格)
            while self.match_token(&[TokenKind::Newline]) {}
            if self.check(TokenKind::RightBracket) {
                break;
            }

            let expr = self.parse_expression()?;
            elements.push(expr);

            // 处理逗号
            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
            // 允许逗号后换行
            while self.match_token(&[TokenKind::Newline]) {}
        }

        let end_token = self.expect(TokenKind::RightBracket)?;
        let span = start_span.to(end_token.span);

        Ok(self.make_node(ExpressionData::Array(elements), span))
    }

    fn parse_return(&mut self) -> ParseResult<Expression> {
        let start_token = self.expect(TokenKind::Return)?;
        // 检查是否有返回值 (根据 Loom 语法，return 后面如果不是换行/分号/Dedent，就是返回值)
        let value = if !self.check(TokenKind::Newline)
            && !self.check(TokenKind::Dedent)
            && !self.check(TokenKind::RightBrace)
            && !self.is_at_end()
        {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        // 注意：parse_expression 可能会吃掉后面的换行吗？通常不会。

        let end_span = value.as_ref().map(|e| e.span).unwrap_or(start_token.span);
        Ok(self.make_node(ExpressionData::Return(value), start_token.span.to(end_span)))
    }

    fn parse_break(&mut self) -> ParseResult<Expression> {
        let token = self.expect(TokenKind::Break)?;
        // Break 也可以带值 (如果 Loom 支持 loop 表达式值)
        Ok(self.make_node(ExpressionData::Break { value: None }, token.span))
    }

    fn parse_continue(&mut self) -> ParseResult<Expression> {
        let token = self.expect(TokenKind::Continue)?;
        Ok(self.make_node(ExpressionData::Continue, token.span))
    }

    fn parse_group_or_tuple(&mut self) -> ParseResult<Expression> {
        let start_token = self.expect(TokenKind::LeftParen)?;
        let start_span = start_token.span;

        // 1. 空元组 ()
        if self.check(TokenKind::RightParen) {
            let end_token = self.advance();
            return Ok(self.make_node(
                ExpressionData::Tuple(Vec::new()),
                start_span.to(end_token.span),
            ));
        }

        // 2. 解析第一个元素
        let first_expr = self.parse_expression()?;

        // 3. 检查是否有逗号
        if self.match_token(&[TokenKind::Comma]) {
            // === 是元组 (a, ...) ===
            let mut elements = vec![first_expr];

            while !self.check(TokenKind::RightParen) && !self.is_at_end() {
                // 处理尾后逗号 (Trailing Comma): (a, )
                if self.check(TokenKind::RightParen) {
                    break;
                }

                // 允许换行 (Python/Loom 风格)
                while self.match_token(&[TokenKind::Newline]) {}

                elements.push(self.parse_expression()?);

                if !self.match_token(&[TokenKind::Comma]) {
                    break;
                }
            }

            let end_token = self.expect(TokenKind::RightParen)?;
            Ok(self.make_node(
                ExpressionData::Tuple(elements),
                start_span.to(end_token.span),
            ))
        } else {
            // === 是分组 (a) ===
            self.expect(TokenKind::RightParen)?;
            // 直接返回内部表达式，不需要包装
            Ok(first_expr)
        }
    }

    /// 解析 For 循环
    /// 语法: for i in 0..10
    ///       for item in list
    fn parse_for(&mut self) -> ParseResult<Expression> {
        let start_span = self.expect(TokenKind::For)?.span;

        // Iterator (i)
        let iter_token = self.expect(TokenKind::Identifier)?;
        let iterator = self.intern_token(iter_token);

        // Keyword 'in'
        self.expect(TokenKind::In)?;

        // Iterable (0..10 或 list)
        // 注意：Loom 的 Range 是二元表达式 (..)，所以 parse_expression 会自动处理 0..10
        let iterable = self.parse_expression()?;

        // Body (Block)
        // 自动处理换行
        if self.check(TokenKind::Newline) {
            self.advance();
        }
        let body = self.parse_block()?;

        let end_span = body.span;

        Ok(self.make_node(
            ExpressionData::For {
                iterator,
                iterable: Box::new(iterable),
                body,
            },
            start_span.to(end_span),
        ))
    }

    /// 解析 While 循环
    /// 语法: while cond
    ///           block
    fn parse_while(&mut self) -> ParseResult<Expression> {
        let start_span = self.expect(TokenKind::While)?.span;

        // Condition
        let condition = self.parse_expression()?;

        // Body
        if self.check(TokenKind::Newline) {
            self.advance();
        }
        let body = self.parse_block()?;

        let end_span = body.span;

        Ok(self.make_node(
            ExpressionData::While {
                condition: Box::new(condition),
                body,
            },
            start_span.to(end_span),
        ))
    }
}

// 放在 parser.rs 或 utils.rs

#[derive(Debug, Clone)]
pub enum ParseIntError {
    Overflow,
    InvalidDigit(char),
    Empty,
}

pub fn parse_int(text: &str) -> Result<u128, ParseIntError> {
    if text.is_empty() {
        return Err(ParseIntError::Empty);
    }

    let mut chars = text.chars();
    let mut radix = 10;

    // 1. 检查前缀 (0x, 0b, 0o)
    if text.starts_with('0') && text.len() > 2 {
        let bytes = text.as_bytes();
        // Lookahead checking the second char
        match bytes.get(1) {
            Some(b'x') | Some(b'X') => {
                radix = 16;
                // 跳过 "0x"
                chars.next(); // 0
                chars.next(); // x
            }
            Some(b'b') | Some(b'B') => {
                radix = 2;
                // 跳过 "0b"
                chars.next();
                chars.next();
            }
            Some(b'o') | Some(b'O') => {
                radix = 8;
                // 跳过 "0o"
                chars.next();
                chars.next();
            }
            _ => {} // 只是普通的 0 开头，比如 0123，保持 10 进制
        }
    }

    // 2. 循环累加
    let mut result: u128 = 0;

    for c in chars {
        // 跳过下划线
        if c == '_' {
            continue;
        }

        let digit = c.to_digit(radix).ok_or(ParseIntError::InvalidDigit(c))?;

        // 3. 溢出检查
        // result * radix
        result = result
            .checked_mul(radix as u128)
            .ok_or(ParseIntError::Overflow)?;

        // result + digit
        result = result
            .checked_add(digit as u128)
            .ok_or(ParseIntError::Overflow)?;
    }

    Ok(result)
}
