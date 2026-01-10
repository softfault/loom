#![allow(unused)]
use crate::utils::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    #[inline(always)]
    pub fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: Span::new(start, end),
        }
    }
}

macro_rules! define_tokens {
    (
        dynamic { $($dynamic_variant:ident),* $(,)? }
        keywords { $($keyword_text:literal => $keyword_variant:ident),* $(,)? }
        symbols { $($symbol_text:literal => $symbol_variant:ident),* $(,)? }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum TokenKind {
            EOF,
            ERROR,
            // 动态 Token (词法分析器根据逻辑生成，而非直接匹配字符串)
            $($dynamic_variant),*,
            // 关键字
            $($keyword_variant),*,
            // 符号
            $($symbol_variant),*,
        }

        impl TokenKind {
            pub fn as_str(&self) -> &'static str {
                match self {
                    TokenKind::EOF => "end of file",
                    TokenKind::ERROR => "error",
                    $(TokenKind::$dynamic_variant => stringify!($dynamic_variant)),*,
                    $(TokenKind::$keyword_variant => $keyword_text),*,
                    $(TokenKind::$symbol_variant => $symbol_text),*,
                }
            }

            pub fn lookup_keyword(text: &str) -> Option<TokenKind> {
                match text {
                    $($keyword_text => Some(TokenKind::$keyword_variant),)*
                    _ => None,
                }
            }
        }
    };
}

define_tokens! {
    dynamic {
        // --- 基础字面量 ---
        Identifier,
        Integer,      // 123
        Float,        // 12.34
        StringLiteral,// "hello"
        CharLiteral,  // 'a'

        // --- 结构控制 (Pythonic / TOML 风格) ---
        // 注意：这三个 Token 通常不对应具体的文本字符，而是由 Lexer 计算空格后生成
        Newline,      // 换行符 (TOML 语句通常以换行结束)
        Indent,       // 缩进增加 (进入代码块)
        Dedent,       // 缩进减少 (退出代码块)
    }

    keywords {
        // --- Loom 基础类型 (根据 Spec) ---
        "int"     => IntType,    // 对应 i64 或平台无关整数
        "float"   => FloatType,  // 对应 f64
        "bool"    => BoolType,
        "str"     => StrType,
        "any"     => AnyType,    // 类似于 TypeScript 的结构化通配符

        // --- 字面量关键字 ---
        "true"    => True,
        "false"   => False,
        "nil"     => Nil,        // 或者 null，用于表示空值

        // --- 模块与引用 ---
        "use"     => Use,        // use std.fs

        // --- 核心关键字 ---
        "self"    => SmallSelf,  // 实例访问：self.host
        "Self"    => BigSelf,    // 约束/类型引用：[T: Self]

        // --- 控制流 ---
        "if"       => If,
        "else"     => Else,
        "for"      => For,
        "in"       => In,
        "while"    => While,
        "break"    => Break,
        "continue" => Continue,
        "return"   => Return,
        "and"      => And,
        "or"       => Or,
        "as"       => As,
    }

    symbols {
        // --- 算术 ---
        "+"   => Plus,
        "-"   => Minus,
        "*"   => Star,
        "/"   => Slash,
        "%"   => Percent,

        // --- 赋值与复合赋值 ---
        "="   => Assign,
        "+="  => PlusAssign,
        "-="  => MinusAssign,
        "*="  => StarAssign,
        "/="  => SlashAssign,
        "%="  => PercentAssign,

        // --- 逻辑 ---
        "!"   => Bang,    // Spec 中使用 if !fs.exists

        // --- 比较 ---
        "=="  => Equal,
        "!="  => NotEqual,
        "<"   => LessThan,
        "<="  => LessEqual,
        ">"   => GreaterThan,
        ">="  => GreaterEqual,

        // --- 标点符号 ---
        "("   => LeftParen,
        ")"   => RightParen,
        "["   => LeftBracket,     // 用于 TOML Section [BaseServer]
        "]"   => RightBracket,
        "{"   => LeftBrace,       // 用于内联对象 { name: str }
        "}"   => RightBrace,

        // --- 特殊符号 ---
        "."   => Dot,             // 成员访问 user.name
        ","   => Comma,           // 分隔符
        ":"   => Colon,           // 类型约束 var: int 或 [Prod: Base]
        ".."  => DotDot,          // 范围 0..limit
        "=>"  => FatArrow,        // 单行函数 add = (a, b) => a + b
    }
}
