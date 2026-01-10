#![allow(unused)]
use crate::token::{Token, TokenKind};
use core::iter::Peekable;
use core::str::Chars;

pub struct Lexer<'a> {
    src: &'a str,
    chars: Peekable<Chars<'a>>,
    current_position: usize,
    start_position: usize,

    // Loom 新增：缩进控制
    indent_stack: Vec<usize>,   // 存储每一层的缩进空格数，初始为 [0]
    pending_tokens: Vec<Token>, // 用于存储 Dedent 时一次性弹出的多个 Token
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            chars: src.chars().peekable(),
            current_position: 0,
            start_position: 0,
            indent_stack: vec![0], // 默认顶层缩进为 0
            pending_tokens: Vec::new(),
        }
    }

    fn make_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, self.start_position, self.current_position)
    }

    // 创建一个特定位置的 Token (用于 Dedent 等不由当前字符生成的 Token)
    fn make_synthetic_token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        Token::new(kind, start, end)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.current_position += c.len_utf8();
        Some(c)
    }

    fn match_char(&mut self, expected: char) -> bool {
        if let Some(&c) = self.chars.peek() {
            if c == expected {
                self.advance();
                return true;
            }
        }
        false
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
}

impl<'a> Lexer<'a> {
    pub fn next_token(&mut self) -> Token {
        // 1. 优先返回队列中的 Token (用于处理连续 Dedent)
        if let Some(token) = self.pending_tokens.pop() {
            return token;
        }

        // 2. 跳过普通空格（但不跳过换行！）
        self.skip_horizontal_whitespace();

        self.start_position = self.current_position;

        let c = match self.peek() {
            Some(c) => c,
            None => {
                // EOF 处理：如果还有缩进，需要自动补 Dedent
                if self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    return self.make_token(TokenKind::Dedent);
                }
                return self.make_token(TokenKind::EOF);
            }
        };

        // 3. 处理换行与缩进逻辑
        if c == '\n' {
            self.advance(); // 吃掉换行
            return self.handle_newline();
        }

        if c == '#' {
            self.skip_comment_line();
            // 注释也是一种“空白”，跳过它是为了读取下一行的缩进
            return self.next_token();
        }

        // 正常 Token 解析
        self.advance(); // 消耗当前字符

        match c {
            // 标识符 (Identifier) & 下划线 (_)
            c if is_ident_start(c) => self.scan_identifier(),

            // 数字 (Number)
            c if c.is_ascii_digit() => self.scan_number(),

            // 字符串
            '"' => self.scan_string(),
            // 字符 (Loom 好像暂时没定义 char 类型，不过保留也行)
            '\'' => self.scan_char(),

            // --- 符号 ---
            '(' => self.make_token(TokenKind::LeftParen),
            ')' => self.make_token(TokenKind::RightParen),
            '{' => self.make_token(TokenKind::LeftBrace),
            '}' => self.make_token(TokenKind::RightBrace),
            '[' => self.make_token(TokenKind::LeftBracket),
            ']' => self.make_token(TokenKind::RightBracket),
            ',' => self.make_token(TokenKind::Comma),
            ':' => self.make_token(TokenKind::Colon),

            // Dot (.)
            '.' => {
                if self.match_char('.') {
                    // case: .. (Range)
                    self.make_token(TokenKind::DotDot)
                } else {
                    // case: . (Access)
                    self.make_token(TokenKind::Dot)
                }
            }

            // Plus (+)
            '+' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::PlusAssign)
                } else {
                    self.make_token(TokenKind::Plus)
                }
            }

            // Minus (-)
            '-' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::MinusAssign)
                } else {
                    self.make_token(TokenKind::Minus)
                }
            }

            // Star (*)
            '*' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::StarAssign)
                } else {
                    self.make_token(TokenKind::Star)
                }
            }

            // Slash (/)
            '/' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::SlashAssign)
                } else {
                    self.make_token(TokenKind::Slash)
                }
            }

            // Mod (%)
            '%' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::PercentAssign)
                } else {
                    self.make_token(TokenKind::Percent)
                }
            }

            // Equal (=)
            '=' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::Equal)
                } else if self.match_char('>') {
                    // case: => (Fat Arrow for inline function)
                    self.make_token(TokenKind::FatArrow)
                } else {
                    self.make_token(TokenKind::Assign)
                }
            }

            // Bang (!)
            '!' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::NotEqual)
                } else {
                    self.make_token(TokenKind::Bang)
                }
            }

            // Less (<)
            '<' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::LessEqual)
                } else {
                    self.make_token(TokenKind::LessThan)
                }
            }

            // Greater (>)
            '>' => {
                if self.match_char('=') {
                    self.make_token(TokenKind::GreaterEqual)
                } else {
                    self.make_token(TokenKind::GreaterThan)
                }
            }

            _ => self.make_token(TokenKind::ERROR),
        }
    }

    // --- 缩进处理核心逻辑 ---

    fn handle_newline(&mut self) -> Token {
        let start_pos = self.current_position;

        // 计算下一行的缩进空格数
        let mut indent_spaces = 0;
        while let Some(&c) = self.chars.peek() {
            if c == ' ' {
                indent_spaces += 1;
                self.advance();
            } else if c == '\t' {
                indent_spaces += 4; // 假设 Tab = 4 空格，或者报错禁止 Tab
                self.advance();
            } else {
                break;
            }
        }

        // 如果紧接着又是换行或注释，忽略这一行（空行不影响缩进）
        if let Some(&next_c) = self.chars.peek() {
            if next_c == '\n' {
                self.advance();
                return self.handle_newline(); // 递归处理下一行
            }
            if next_c == '/' && self.peek_next_is('/') {
                self.skip_comment_line();
                // 注释行也不影响缩进
                return self.handle_newline();
            }
        }

        let current_indent = *self.indent_stack.last().unwrap();

        if indent_spaces > current_indent {
            // 缩进增加 -> Indent Token
            self.indent_stack.push(indent_spaces);
            self.make_synthetic_token(TokenKind::Indent, start_pos, self.current_position)
        } else if indent_spaces < current_indent {
            // 缩进减少 -> 可能产生多个 Dedent Token
            while let Some(&top) = self.indent_stack.last() {
                if top > indent_spaces {
                    self.indent_stack.pop();
                    // 这里我们先把 Token 存入 pending，因为 next_token 只能返回一个
                    // 注意：pending 是栈，先进后出，所以我们 push 进去，pop 出来正好顺序对
                    // 但这里全是 Dedent，顺序无所谓。
                    // 最后一个 Dedent 我们直接返回，剩下的存 pending
                    if *self.indent_stack.last().unwrap() > indent_spaces {
                        self.pending_tokens.push(self.make_synthetic_token(
                            TokenKind::Dedent,
                            start_pos,
                            self.current_position,
                        ));
                    } else if *self.indent_stack.last().unwrap() == indent_spaces {
                        // 匹配到了，返回最后一个 Dedent
                        return self.make_synthetic_token(
                            TokenKind::Dedent,
                            start_pos,
                            self.current_position,
                        );
                    } else {
                        // 缩进不对齐错误！(例如 4 -> 2 -> 3)
                        return self.make_token(TokenKind::ERROR);
                    }
                } else {
                    break;
                }
            }
            // 理论上上面会 return，如果不匹配会走 Error
            self.make_token(TokenKind::ERROR)
        } else {
            // 缩进不变 -> Newline Token
            self.make_synthetic_token(TokenKind::Newline, start_pos, self.current_position)
        }
    }

    // --- 辅助函数 ---

    fn skip_horizontal_whitespace(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment_line(&mut self) {
        // 假设注释是 //
        self.advance(); // /
        self.advance(); // /
        while let Some(&c) = self.chars.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    // --- 以下保持大部分原有逻辑，微调 ---

    fn scan_identifier(&mut self) -> Token {
        let start = self.start_position; // 修正：使用 start_position
        // 注意：首字符已经被 advance 消耗了，所以不需要在这里 advance
        // 但为了获取完整字符串，我们需要从 src 中切片

        while let Some(&c) = self.chars.peek() {
            if is_ident_continue(c) {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.src[self.start_position..self.current_position];

        // 关键字查找
        let kind = TokenKind::lookup_keyword(text).unwrap_or(TokenKind::Identifier);
        self.make_token(kind)
    }

    pub fn scan_number(&mut self) -> Token {
        self.consume_digits(10);
        // 简单处理 float
        if self.peek_is('.') && self.peek_next_is_digit() {
            self.advance();
            self.consume_digits(10);
            return self.make_token(TokenKind::Float);
        }
        self.make_token(TokenKind::Integer)
    }

    fn consume_digits(&mut self, radix: u32) {
        while let Some(&c) = self.chars.peek() {
            if c.is_digit(radix) || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn peek_is(&mut self, expected: char) -> bool {
        self.peek() == Some(expected)
    }

    fn peek_next_is(&self, expected: char) -> bool {
        let mut lookahead = self.chars.clone();
        lookahead.next();
        lookahead.next() == Some(expected)
    }

    fn peek_next_is_digit(&self) -> bool {
        let mut lookahead = self.chars.clone();
        lookahead.next();
        if let Some(c) = lookahead.next() {
            c.is_ascii_digit()
        } else {
            false
        }
    }

    fn scan_string(&mut self) -> Token {
        while let Some(&c) = self.chars.peek() {
            match c {
                '"' => {
                    self.advance();
                    return self.make_token(TokenKind::StringLiteral);
                }
                '\\' => {
                    self.advance();
                    self.advance();
                } // 转义
                '\n' => return self.make_token(TokenKind::ERROR), // 禁止跨行字符串
                _ => {
                    self.advance();
                }
            }
        }
        self.make_token(TokenKind::ERROR)
    }

    fn scan_char(&mut self) -> Token {
        // 简单实现
        if let Some(c) = self.advance() {
            if c == '\'' {
                return self.make_token(TokenKind::ERROR);
            } // 空 char
            if c == '\\' {
                self.advance();
            }
        }
        if self.match_char('\'') {
            self.make_token(TokenKind::CharLiteral) // 需要在 TokenKind 加这个或者复用 Integer
        } else {
            self.make_token(TokenKind::ERROR)
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
