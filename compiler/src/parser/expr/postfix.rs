// postfix.rs
use crate::ast::*;
use crate::parser::{ParseError, ParseResult, Parser};
use crate::token::TokenKind;

impl<'a> Parser<'a> {
    pub fn parse_postfix(&mut self, base: Expression) -> ParseResult<Expression> {
        let token = self.peek();

        match token.kind {
            // Call: func(a, b: 1)
            TokenKind::LeftParen => {
                let args = self.parse_call_args()?;
                let end_span = self.previous_span();
                Ok(self.make_node(
                    ExpressionData::Call {
                        callee: Box::new(base.clone()), // base 移入
                        generic_args: vec![],           // 普通调用无泛型
                        args,
                    },
                    base.span.to(end_span),
                ))
            }

            // Member: obj.prop
            TokenKind::Dot => {
                self.advance(); // eat .
                let name_token = self.expect(TokenKind::Identifier)?;
                let name = self.intern_token(name_token);

                Ok(self.make_node(
                    ExpressionData::FieldAccess {
                        target: Box::new(base.clone()),
                        field: name,
                    },
                    base.span.to(name_token.span),
                ))
            }

            // Index: arr[i] (暂时 Spec 没提，但如果有的话)
            TokenKind::LeftBracket => {
                self.advance(); // 吃掉 '['

                // 解析索引表达式 (可能是整数、变量，或者是 Range 0..1)
                let index = self.parse_expression()?;

                let end_token = self.expect(TokenKind::RightBracket)?;
                let span = base.span.to(end_token.span);

                Ok(self.make_node(
                    ExpressionData::Index {
                        target: Box::new(base), //原本的 base 变成了 target
                        index: Box::new(index),
                    },
                    span,
                ))
            }

            _ => Ok(base),
        }
    }

    /// 解析调用参数 (支持位置参数和命名参数)
    /// (1, 2, width: 100)
    fn parse_call_args(&mut self) -> ParseResult<Vec<CallArg>> {
        self.expect(TokenKind::LeftParen)?;
        let mut args = Vec::new();

        while !self.check(TokenKind::RightParen) && !self.is_at_end() {
            let start_span = self.peek().span;

            // 检查是否是命名参数: ident : expr
            // 需要 Lookahead 2: IDENT COLON ...
            let mut name = None;
            if self.check(TokenKind::Identifier) && self.check_nth(1, TokenKind::Colon) {
                let name_token = self.advance();
                name = Some(self.intern_token(name_token));
                self.advance(); // eat Colon
            }

            let value = self.parse_expression()?;
            let end_span = value.span;

            args.push(self.make_node(CallArgData { name, value }, start_span.to(end_span)));

            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }

        self.expect(TokenKind::RightParen)?;
        Ok(args)
    }

    // 专门处理 run<T>... 这种情况
    pub fn parse_postfix_generic(&mut self, base: Expression) -> ParseResult<Expression> {
        // 这里我们确定它是泛型了
        let args = self.parse_generic_args_list()?; // 解析 <T, U>

        // 泛型后面通常紧跟调用 ()
        // 我们这里将其作为 Call 的一部分返回，或者先返回一个 GenericInstance 节点
        // 根据 AST 结构，Call 节点包含 generic_args

        if self.check(TokenKind::LeftParen) {
            let call_args = self.parse_call_args()?; // 解析 (a, b)
            let end_span = self.previous_span();

            Ok(self.make_node(
                ExpressionData::Call {
                    callee: Box::new(base),
                    generic_args: args, // 填入泛型参数
                    args: call_args,
                },
                // span 需要合并
                // 注意：这里计算 span 有点麻烦，因为 base 的 span 到 end_span
                // 暂时用 end_span (即 ')')
                self.previous_span(), // 修正 Span 逻辑
            ))
        } else {
            // 只是获取泛型对象？比如 func_ptr = my_func<int>
            // 如果允许这种情况
            let end_span = self.previous_span();
            // 这里需要 AST 支持 ExpressionData::GenericInst 或者类似的东西
            // 暂时假设我们只支持泛型调用
            Err(ParseError {
                expected: "(".into(),
                found: self.peek().kind,
                span: self.peek().span,
                message: "Generic arguments must be followed by function call arguments".into(),
            })
        }
    }

    // 简单的 <T, U> 解析器
    fn parse_generic_args_list(&mut self) -> ParseResult<Vec<TypeRef>> {
        self.expect(TokenKind::LessThan)?;
        let mut args = Vec::new();
        while !self.check(TokenKind::GreaterThan) {
            args.push(self.parse_type()?);
            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }
        self.expect(TokenKind::GreaterThan)?;
        Ok(args)
    }
}
