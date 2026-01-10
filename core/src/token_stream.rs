// src/token_stream.rs

use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};
use crate::utils::Span;
use std::collections::VecDeque;

pub struct TokenStream<'a> {
    lexer: Lexer<'a>,
    buffer: VecDeque<Token>,
    pub last_span: Span,
}

impl<'a> TokenStream<'a> {
    // [修改] 构造函数不需要 base_offset
    pub fn new(lexer: Lexer<'a>) -> Self {
        Self {
            lexer,
            buffer: VecDeque::new(),
            last_span: Span::new(0, 0),
        }
    }

    pub fn fill(&mut self, n: usize) {
        while self.buffer.len() <= n {
            let tok = self.lexer.next_token();

            self.buffer.push_back(tok);
            if tok.kind == TokenKind::EOF {
                break;
            }
        }
    }

    // peek 和 advance 保持不变...
    pub fn peek(&mut self, n: usize) -> Token {
        self.fill(n);
        *self
            .buffer
            .get(n)
            .unwrap_or_else(|| self.buffer.back().unwrap())
    }

    pub fn advance(&mut self) -> Token {
        self.fill(0);
        let tok = self
            .buffer
            .pop_front()
            .unwrap_or_else(|| Token::new(TokenKind::EOF, self.last_span.end, self.last_span.end));
        self.last_span = tok.span;
        tok
    }
}
