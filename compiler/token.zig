const std = @import("std");
const Span = @import("utils.zig").Span;

/// 头啃
pub const Token = struct {
    tag: TokenType,
    span: Span,

    /// 判断是否是宏调用符号
    pub fn isMacroBang(self: Token) bool {
        return self.tag == .Bang;
    }

    pub const keywords = std.StaticStringMap(TokenType).initComptime(.{
        .{ "fn", .Fn },
        .{ "let", .Let },
        .{ "mut", .Mut },
        .{ "const", .Const },
        .{ "struct", .Struct },
        .{ "enum", .Enum },
        .{ "union", .Union },
        .{ "if", .If },
        .{ "else", .Else },
        .{ "match", .Match },
        .{ "for", .For },
        .{ "break", .Break },
        .{ "continue", .Continue },
        .{ "return", .Return },
        .{ "pub", .Pub },
        .{ "extern", .Extern },
        .{ "use", .Use },
        .{ "impl", .Impl },
        .{ "trait", .Trait },
        .{ "type", .Type },
        .{ "macro", .Macro },
        .{ "true", .True },
        .{ "false", .False },
        .{ "undef", .Undef },
        .{ "self", .SelfValue },
        .{ "Self", .SelfType },
        .{ "as", .As },
        .{ "defer", .Defer },
        .{ "and", .And },
        .{ "or", .Or },
        .{ "null", .Null },
        .{ "_", .Underscore },
    });
};

/// 头啃太普
pub const TokenType = enum {
    // === 标识符与字面量 ===
    Identifier, // abc, my_var
    IntLiteral, // 123, 0xFF
    FloatLiteral, // 3.14
    StringLiteral, // "hello"
    CharLiteral, // 'a'

    // === 关键字 ===
    Fn,
    Let,
    Mut,
    Const,
    Struct,
    Enum,
    Union,
    If,
    Else,
    Match,
    For,
    Break,
    Continue,
    Return,
    Pub,
    Extern,
    Use,
    Impl,
    Trait,
    Type,
    Macro,
    True,
    False,
    Undef,
    SelfValue,
    SelfType,
    As,
    Defer,
    And,
    Or,
    Null,
    Underscore,

    // === 运算符与符号 ===

    // + - * / % #
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Hash,

    // == ! !=
    Equal,
    Bang,
    NotEqual,

    // > >= < <=
    GreaterThan,
    GreaterEqual,
    LessThan,
    LessEqual,

    // & | ^ ~
    Ampersand,
    Pipe,
    Caret,
    Tilde,

    // << >>
    LShift,
    RShift,

    // = 家族
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    AmpersandAssign,
    PipeAssign,
    CaretAssign,
    LShiftAssign,
    RShiftAssign,

    // === 标点 ===

    Dot, // .
    DotDot, // ..
    DotQuestion, // .?
    DotLessThan, // .<
    DotAmpersand, // .&
    Ellipsis, // ...
    Comma, // ,
    Colon, // :
    Semicolon, // ;
    Question, // ?
    Dollar, // $

    // ( )
    LParen,
    RParen,

    // { }
    LBrace,
    RBrace,

    // [ ]
    LBracket,
    RBracket,

    // =>
    Arrow,

    // === 特殊 ===
    Eof, // 文件结束
    Illegal, // 无法识别的字符
};
