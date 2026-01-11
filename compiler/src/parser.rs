#![allow(unused)]

mod expr;
mod top;
mod ty;

use crate::ast::*;
use crate::context::Context;
use crate::lexer::Lexer;
use crate::source::FileId;
use crate::token::{Token, TokenKind};
use crate::utils::{Interner, Node, NodeId, Span, Symbol};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub expected: String,
    pub found: TokenKind,
    pub span: Span,
    pub message: String,
}

pub type ParseResult<T> = Result<T, ParseError>;

pub struct Parser<'a> {
    source: &'a str,
    stream: crate::token_stream::TokenStream<'a>,
    pub errors: Vec<ParseError>, // 收集错误

    previous_kind: TokenKind,
    node_id_counter: u32,

    pub file_id: FileId,

    interner: &'a mut Interner,
}

impl<'a> Parser<'a> {
    pub fn new(
        source: &'a str,
        lexer: Lexer<'a>,
        file_id: FileId, // 传入 FileId
        interner: &'a mut Interner,
    ) -> Self {
        Self {
            source,
            // TokenStream 不再需要 offset
            stream: crate::token_stream::TokenStream::new(lexer),
            errors: Vec::new(),
            previous_kind: TokenKind::EOF,
            node_id_counter: 0,
            file_id,
            interner,
        }
    }

    /// 创建 AST 节点 (自动分配 ID)
    pub fn make_node<T>(&mut self, data: T, span: Span) -> Node<T> {
        let id = self.next_id();
        Node::new(id, span, data)
    }

    /// 分配 ID
    fn next_id(&mut self) -> NodeId {
        let id = self.node_id_counter;
        self.node_id_counter += 1;
        NodeId(id)
    }

    // --- 字符串驻留辅助 ---

    /// 将当前/指定 Token 的文本 intern 为 Symbol
    pub fn intern_token(&mut self, token: Token) -> Symbol {
        let text = self.text(token).to_string(); // text() 返回 &str，这里转 String 供 intern
        self.interner.intern(&text)
    }

    // --- Token 检查与消费 (TokenStream Wrapper) ---

    pub fn peek(&mut self) -> Token {
        self.stream.peek(0)
    }

    pub fn peek_nth(&mut self, n: usize) -> Token {
        self.stream.peek(n)
    }

    pub fn check(&mut self, kind: TokenKind) -> bool {
        self.peek().kind == kind
    }

    pub fn check_nth(&mut self, n: usize, kind: TokenKind) -> bool {
        self.stream.peek(n).kind == kind
    }

    pub fn is_at_end(&mut self) -> bool {
        self.peek().kind == TokenKind::EOF
    }

    pub fn advance(&mut self) -> Token {
        let tok = self.stream.advance();
        self.previous_kind = tok.kind;
        tok
    }

    pub fn consume(&mut self, kind: TokenKind) -> Option<Token> {
        if self.check(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// 强制匹配，失败则报错
    pub fn expect(&mut self, kind: TokenKind) -> ParseResult<Token> {
        if let Some(token) = self.consume(kind) {
            Ok(token)
        } else {
            let current = self.peek();
            // 特殊处理：如果期望 Indent 但没遇到，可能意味着用户没缩进
            let msg = if kind == TokenKind::Indent {
                "Expected indentation (new block)".to_string()
            } else {
                format!(
                    "Expected '{}', but found '{}'",
                    kind.as_str(),
                    current.kind.as_str()
                )
            };

            Err(ParseError {
                expected: kind.as_str().to_string(),
                found: current.kind,
                span: current.span,
                message: msg,
            })
        }
    }

    pub fn match_token(&mut self, kinds: &[TokenKind]) -> bool {
        for &kind in kinds {
            if self.check(kind) {
                self.advance();
                return true;
            }
        }
        false
    }

    /// 获取 Token 文本 (处理 Offset)
    pub fn text(&self, token: Token) -> &'a str {
        let start = token.span.start;
        let end = token.span.end;

        // 安全检查，防止 panic
        if start >= self.source.len() || end > self.source.len() || start > end {
            return "";
        }

        // 直接切片，因为 source 就是当前文件的内容，span 就是相对当前文件的位置
        &self.source[start..end]
    }

    // --- 辅助功能 ---

    pub fn previous_span(&self) -> Span {
        self.stream.last_span
    }

    /// Loom 的 Synchronize 逻辑
    pub fn synchronize(&mut self) {
        self.advance();

        while !self.is_at_end() {
            // 遇到这些关键字，说明可能是一个新定义的开始，可以在这里恢复解析
            match self.peek().kind {
                TokenKind::Class | TokenKind::Fn | TokenKind::Use => return,
                _ => {}
            }

            self.advance();
        }
    }

    // --- 字符串 Unescape ---
    pub fn unescape_string(&self, raw: &str) -> String {
        let mut result = String::new();
        let mut chars = raw.chars().peekable();
        let inner = if raw.len() >= 2 && (raw.starts_with('"') || raw.starts_with('\'')) {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };

        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some('\'') => result.push('\''),
                    Some('0') => result.push('\0'),
                    Some('u') => {
                        if chars.peek() == Some(&'{') {
                            chars.next();
                            let mut hex_string = String::new();
                            while let Some(&ch) = chars.peek() {
                                if ch == '}' {
                                    chars.next();
                                    break;
                                }
                                hex_string.push(chars.next().unwrap());
                            }
                            if let Ok(code) = u32::from_str_radix(&hex_string, 16) {
                                if let Some(uni_char) = std::char::from_u32(code) {
                                    result.push(uni_char);
                                }
                            }
                        } else {
                            result.push('u');
                        }
                    }
                    Some(other) => result.push(other),
                    None => break,
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// 解析入口：Program
    pub fn parse_program(&mut self) -> ParseResult<Program> {
        let start_span = self.peek().span;
        let mut definitions = Vec::new();

        while !self.is_at_end() {
            // 跳过空行
            while self.match_token(&[TokenKind::Newline]) {
                continue;
            }
            if self.is_at_end() {
                break;
            }

            // 分流逻辑
            if self.check(TokenKind::Use) {
                let use_stmt = self.parse_use_statement()?;
                definitions.push(TopLevelItem::Use(use_stmt));
            } else if self.check(TokenKind::Class) {
                // 处理类定义: class Dog ...
                let item = self.parse_class_definition()?;
                definitions.push(item);
            } else if self.check(TokenKind::Fn) {
                // 处理顶层函数: fn main() ...
                let item = self.parse_function_definition()?;
                definitions.push(item);
            } else if self.check(TokenKind::Identifier) {
                let field = self.parse_top_level_field()?;
                definitions.push(TopLevelItem::Field(field));
            } else {
                // 错误处理
                let err_token = self.peek();
                self.errors.push(ParseError {
                    expected: "[ or use or identifier".into(),
                    found: err_token.kind,
                    span: err_token.span,
                    message: "Expected definition, variable, or use statement".into(),
                });
                self.synchronize();
            }
        }

        let end_span = self.previous_span();
        Ok(Program {
            definitions,
            span: start_span.to(end_span),
        })
    }
}
