const std = @import("std");
const Token = @import("token.zig").Token;
const TokenType = @import("token.zig").TokenType;
const Span = @import("utils.zig").Span;

pub const Lexer = struct {
    source: []const u8,
    start: usize = 0, // 当前 Token 的起始位置
    current: usize = 0, // 扫描探针的当前位置

    pub fn init(source: []const u8) Lexer {
        return .{
            .source = source,
        };
    }

    /// 获取下一个 Token
    pub fn next(self: *Lexer) Token {
        self.skipWhitespace();

        self.start = self.current;

        const char = self.advance() orelse return self.makeToken(.Eof);

        return switch (char) {
            // 标识符或关键字 (a-z, A-Z, _)
            'a'...'z', 'A'...'Z', '_' => self.scanIdentifier(),

            // 数字 (0-9)
            '0'...'9' => self.scanNumber(),

            // 字符串
            '"' => self.scanString(),

            // 字符
            '\'' => self.scanChar(),

            // 运算符和标点
            '(' => self.makeToken(.LParen),
            ')' => self.makeToken(.RParen),
            '{' => self.makeToken(.LBrace),
            '}' => self.makeToken(.RBrace),
            '[' => self.makeToken(.LBracket),
            ']' => self.makeToken(.RBracket),
            ',' => self.makeToken(.Comma),
            ';' => self.makeToken(.Semicolon),
            ':' => self.makeToken(.Colon),
            '#' => self.makeToken(.Hash),
            '?' => self.makeToken(.Question),
            '$' => self.makeToken(.Dollar),
            '@' => self.makeToken(.At),
            // Dot 家族处理
            '.' => {
                // 1. 检查 ...
                if (self.match('.')) {
                    // 检查是否是 ... (变长参数)
                    if (self.match('.')) {
                        return self.makeToken(.Ellipsis);
                    }
                    // 检查是否是 ..= (范围匹配) [新增]
                    else if (self.match('=')) {
                        return self.makeToken(.DotDotEqual);
                    }
                    // 否则就是普通的 ..
                    return self.makeToken(.DotDot);
                }
                // 2. 检查 .?
                else if (self.match('?')) {
                    return self.makeToken(.DotQuestion);
                }
                // 3. 检查 .*
                else if (self.match('*')) {
                    return self.makeToken(.DotStar);
                }
                // 4. 检查 .<
                else if (self.match('<')) {
                    return self.makeToken(.DotLessThan);
                }
                // 5. 普通点 .
                else {
                    return self.makeToken(.Dot);
                }
            },

            '+' => self.matchAssign(.Plus, .PlusAssign),
            '-' => self.matchAssign(.Minus, .MinusAssign),
            '*' => self.matchAssign(.Star, .StarAssign),
            '%' => self.matchAssign(.Percent, .PercentAssign),
            '/' => {
                // 1. 检查是否是单行注释 //
                if (self.match('/')) {
                    self.skipCommentLine();
                    return self.next(); // 递归调用，寻找下一个有效 Token
                }
                // 2. 检查是否是多行注释 /*
                else if (self.match('*')) {
                    // 进入这里时，已经消耗了 "/*"
                    // 所以 skipCommentBlock 内部 depth 初始为 1
                    self.skipCommentBlock();
                    return self.next(); // 递归调用
                }
                // 3. 检查是否是除法赋值 /=
                else if (self.match('=')) {
                    return self.makeToken(.SlashAssign);
                }
                // 4. 普通除号 /
                else {
                    return self.makeToken(.Slash);
                }
            },

            '=' => if (self.match('=')) self.makeToken(.Equal) else if (self.match('>')) self.makeToken(.Arrow) else self.makeToken(.Assign),
            '!' => if (self.match('=')) self.makeToken(.NotEqual) else self.makeToken(.Bang),
            '<' => {
                // 检查是否是左移 <<
                if (self.match('<')) {
                    // 检查是否是左移赋值 <<=
                    if (self.match('=')) return self.makeToken(.LShiftAssign);
                    return self.makeToken(.LShift);
                }
                // 检查是否是小于等于 <=
                if (self.match('=')) return self.makeToken(.LessEqual);
                // 否则就是小于 <
                return self.makeToken(.LessThan);
            },

            '>' => {
                // 检查是否是右移 >>
                if (self.match('>')) {
                    // 检查是否是右移赋值 >>=
                    if (self.match('=')) return self.makeToken(.RShiftAssign);
                    return self.makeToken(.RShift);
                }
                // 检查是否是大于等于 >=
                if (self.match('=')) return self.makeToken(.GreaterEqual);
                // 否则就是大于 >
                return self.makeToken(.GreaterThan);
            },

            // 位运算
            '&' => self.matchAssign(.Ampersand, .AmpersandAssign),
            '|' => self.matchAssign(.Pipe, .PipeAssign),
            '^' => self.matchAssign(.Caret, .CaretAssign),
            '~' => self.makeToken(.Tilde),

            else => self.makeToken(.Illegal),
        };
    }

    // === 核心扫描逻辑 ===

    fn scanIdentifier(self: *Lexer) Token {
        while (isAlphaNumeric(self.peek())) {
            _ = self.advance();
        }

        const text = self.source[self.start..self.current];
        // 查表
        const tag = Token.keywords.get(text) orelse .Identifier;
        return self.makeToken(tag);
    }

    fn scanNumber(self: *Lexer) Token {
        // 1. 处理进制前缀 (0x, 0b, 0o)
        // 只有以 '0' 开头才可能是进制前缀
        if (self.source[self.start] == '0') {
            // start 指向 '0'，current 已经在 '0' 后面了，所以 peek 看的是第二个字符
            const next_char = self.peek();

            switch (next_char) {
                'x', 'X' => {
                    _ = self.advance(); // 吃掉 'x'
                    self.consumeDigits(16); // 扫描十六进制
                    return self.makeToken(.IntLiteral);
                },
                'b', 'B' => {
                    _ = self.advance(); // 吃掉 'b'
                    self.consumeDigits(2); // 扫描二进制
                    return self.makeToken(.IntLiteral);
                },
                'o', 'O' => {
                    _ = self.advance(); // 吃掉 'o'
                    self.consumeDigits(8); // 扫描八进制
                    return self.makeToken(.IntLiteral);
                },
                else => {
                    // 只是一个普通的 0，或者 0.xxxx，或者 0123
                    // 继续往下走，进入十进制逻辑
                },
            }
        }

        // 2. 扫描整数部分 (十进制)
        self.consumeDigits(10);

        // 3. 处理小数部分 (Float)
        // 关键逻辑：如果是 '.'，必须确认 '.' 后面紧跟着数字，才算是浮点数。
        // 否则可能是 1.method() 或者是 1..10 (Range)
        if (self.peek() == '.' and isDigit(self.peekNext())) {
            _ = self.advance(); // 吃掉 '.'
            self.consumeDigits(10); // 扫描小数部分

            // 扫描完小数后，还可以继续跟指数部分，如 1.2e10
            _ = self.tryScanExponent();
            return self.makeToken(.FloatLiteral);
        }

        // 4. 处理没有小数点的指数部分 (如 1e10)
        // 这也是浮点数
        if (self.tryScanExponent()) {
            return self.makeToken(.FloatLiteral);
        }

        // 既没有小数点，也没有指数，就是普通的整数
        return self.makeToken(.IntLiteral);
    }

    /// 尝试扫描指数部分 (e/E)，如果有返回 true
    fn tryScanExponent(self: *Lexer) bool {
        const c = self.peek();
        if (c == 'e' or c == 'E') {
            _ = self.advance(); // 吃掉 'e'

            // 指数部分可以有正负号: 1e-10, 1e+5
            const next_c = self.peek();
            if (next_c == '+' or next_c == '-') {
                _ = self.advance();
            }

            self.consumeDigits(10);
            return true;
        }
        return false;
    }

    /// 通用的数字消费函数，支持下划线分隔符
    fn consumeDigits(self: *Lexer, radix: u8) void {
        while (true) {
            const c = self.peek();
            if (c == '_') {
                _ = self.advance();
                continue;
            }

            // 根据进制判断是否是有效数字
            const is_valid = switch (radix) {
                2 => isBinDigit(c),
                8 => isOctDigit(c),
                10 => isDigit(c),
                16 => isHexDigit(c),
                else => false, // unreachable
            };

            if (is_valid) {
                _ = self.advance();
            } else {
                break;
            }
        }
    }

    fn scanString(self: *Lexer) Token {
        while (true) {
            const char = self.peek();
            switch (char) {
                0 => return self.makeToken(.Illegal), // 未闭合就结束
                '"' => {
                    _ = self.advance(); // 吞掉右引号
                    break;
                },
                '\\' => {
                    _ = self.advance(); // 跳过转义
                    _ = self.advance();
                },
                else => {
                    _ = self.advance();
                },
            }
        }
        return self.makeToken(.StringLiteral);
    }

    fn scanChar(self: *Lexer) Token {
        // 刚吃掉了左边的单引号 '，现在处于字符内容的第一个字节
        const c = self.peek();

        // 1. 处理转义字符 (以 \ 开头)
        if (c == '\\') {
            _ = self.advance(); // 吃掉反斜杠 '\'

            const escaped = self.peek();
            switch (escaped) {
                // 简单单字符转义
                'n', 'r', 't', '\\', '\'', '\"', '0' => {
                    _ = self.advance();
                },

                // 十六进制转义: \xNN
                'x' => {
                    _ = self.advance(); // 吃掉 'x'
                    // 必须严格吃掉两个十六进制位
                    if (!self.consumeHexDigits(2)) return self.makeToken(.Illegal);
                },

                // Unicode 转义: \u{...}
                'u' => {
                    _ = self.advance(); // 吃掉 'u'
                    if (self.peek() != '{') return self.makeToken(.Illegal);
                    _ = self.advance(); // 吃掉 '{'

                    // 吃掉中间的十六进制位，直到 '}'
                    // 限制最大长度防止死循环，比如 Unicode 最大 6 位 hex
                    var length: usize = 0;
                    while (isHexDigit(self.peek())) {
                        _ = self.advance();
                        length += 1;
                        if (length > 6) return self.makeToken(.Illegal);
                    }

                    if (self.peek() != '}') return self.makeToken(.Illegal);
                    _ = self.advance(); // 吃掉 '}'
                },

                else => return self.makeToken(.Illegal), // 未知的转义，比如 \q
            }
        }
        // 2. 处理普通字符 (包括 UTF-8 多字节字符)
        else if (c != '\'' and c != 0) {
            const len = std.unicode.utf8ByteSequenceLength(c) catch {
                return self.makeToken(.Illegal);
            };

            // 吃掉该字符的所有字节
            var i: usize = 0;
            while (i < len) : (i += 1) {
                _ = self.advance();
            }
        }
        // 3. 空字符 '' 或者直接遇到 EOF
        else {
            return self.makeToken(.Illegal);
        }

        // 4. 必须以单引号闭合
        if (self.match('\'')) {
            return self.makeToken(.CharLiteral);
        }

        return self.makeToken(.Illegal);
    }

    fn consumeHexDigits(self: *Lexer, count: usize) bool {
        var i: usize = 0;
        while (i < count) : (i += 1) {
            if (isHexDigit(self.peek())) {
                _ = self.advance();
            } else {
                return false;
            }
        }
        return true;
    }

    // === 辅助工具  ===

    // 前进一格并返回字符
    fn advance(self: *Lexer) ?u8 {
        if (self.current >= self.source.len) return null;
        const c = self.source[self.current];
        self.current += 1;
        return c;
    }

    // 仅查看当前字符
    fn peek(self: *Lexer) u8 {
        if (self.current >= self.source.len) return 0;
        return self.source[self.current];
    }

    // 查看下一个字符 (Lookahead 1)
    fn peekNext(self: *Lexer) u8 {
        if (self.current + 1 >= self.source.len) return 0;
        return self.source[self.current + 1];
    }

    // 匹配当前字符，如果匹配则前进
    fn match(self: *Lexer, expected: u8) bool {
        if (self.current >= self.source.len) return false;
        if (self.source[self.current] != expected) return false;
        self.current += 1;
        return true;
    }

    // 语法糖：匹配 =, 如 +=, -=
    fn matchAssign(self: *Lexer, single: TokenType, double: TokenType) Token {
        if (self.match('=')) {
            return self.makeToken(double);
        }
        return self.makeToken(single);
    }

    fn makeToken(self: *Lexer, tag: TokenType) Token {
        return .{
            .tag = tag,
            .span = Span.new(self.start, self.current),
        };
    }

    fn skipWhitespace(self: *Lexer) void {
        while (true) {
            const c = self.peek();
            switch (c) {
                ' ', '\t', '\r', '\n' => _ = self.advance(),
                else => break,
            }
        }
    }

    fn skipCommentLine(self: *Lexer) void {
        while (self.peek() != '\n' and self.peek() != 0) {
            _ = self.advance();
        }
    }

    fn skipCommentBlock(self: *Lexer) void {
        var depth: usize = 1;

        while (depth > 0) {
            const c = self.peek();

            // 1. 检查是否到达文件末尾 (EOF)
            // 如果文件结束了注释还没闭合，这是一个错误，
            // 这里直接停止，记得在Parser中检查和处理报错
            if (c == 0 and self.current >= self.source.len) {
                return;
            }

            // 2. 检查嵌套开始 /*
            if (c == '/' and self.peekNext() == '*') {
                // 吃掉 '/'
                _ = self.advance();
                // 吃掉 '*'
                _ = self.advance();
                depth += 1;
                continue;
            }

            // 3. 检查嵌套结束 */
            if (c == '*' and self.peekNext() == '/') {
                // 吃掉 '*'
                _ = self.advance();
                // 吃掉 '/'
                _ = self.advance();
                depth -= 1;
                continue;
            }

            // 4. 普通字符，跳过
            _ = self.advance();
        }
    }
};

fn isAlphaNumeric(c: u8) bool {
    return isAlpha(c) or isDigit(c);
}

fn isAlpha(c: u8) bool {
    return (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or c == '_';
}

fn isDigit(c: u8) bool {
    return c >= '0' and c <= '9';
}

fn isHexDigit(c: u8) bool {
    return (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f') or (c >= 'A' and c <= 'F');
}

fn isBinDigit(c: u8) bool {
    return c == '0' or c == '1';
}

fn isOctDigit(c: u8) bool {
    return c >= '0' and c <= '7';
}

// === 测试区 ===

// 核心测试辅助函数
fn expectTokens(source: []const u8, expected_tags: []const TokenType) !void {
    var lex = Lexer.init(source);

    for (expected_tags) |expected_tag| {
        const next_token = lex.next();
        // 验证 Token 类型是否匹配
        try std.testing.expectEqual(expected_tag, next_token.tag);
    }

    // 最后必须确保读到的是 EOF
    const end_token = lex.next();
    try std.testing.expectEqual(TokenType.Eof, end_token.tag);
}

test "Lexer - Basic Symbols" {
    try expectTokens("= + ( ) { }", &.{ .Assign, .Plus, .LParen, .RParen, .LBrace, .RBrace });

    try expectTokens("#len ?opt val.? val.* func.<T> ... @bitcast 1..=10", &.{
        .Hash,
        .Identifier,
        .Question,
        .Identifier,
        .Identifier,
        .DotQuestion,
        .Identifier,
        .DotStar, // val.*
        .Identifier,
        .DotLessThan,
        .Identifier,
        .GreaterThan,
        .Ellipsis,
        .At, // @
        .Identifier, // bitcast
        .IntLiteral,
        .DotDotEqual, // ..=
        .IntLiteral,
    });
}

test "Lexer - Numbers" {
    try expectTokens(
        \\123 
        \\123_456 
        \\0xDEAD_BEEF 
        \\0b1010
        \\3.14 
        \\0.5 
        \\1e10 
        \\2.5e-3
    , &.{
        .IntLiteral, // 123
        .IntLiteral, // 123_456
        .IntLiteral, // 0xDEAD_BEEF
        .IntLiteral, // 0b1010
        .FloatLiteral, // 3.14
        .FloatLiteral, // 0.5
        .FloatLiteral, // 1e10
        .FloatLiteral, // 2.5e-3
    });
}

test "Lexer - Range vs Float" {
    // 1..5 应该是 Int, DotDot, Int
    // 1.5 应该是 Float
    try expectTokens("1..5", &.{ .IntLiteral, .DotDot, .IntLiteral });
    try expectTokens("1.5", &.{.FloatLiteral});
}

test "Lexer - Chars" {
    try expectTokens(
        \\'a' '\n' '\'' '\\' '\xAF' '\u{1F600}'
    , &.{
        .CharLiteral,
        .CharLiteral,
        .CharLiteral,
        .CharLiteral,
        .CharLiteral,
        .CharLiteral,
    });
}

test "Lexer - Strings" {
    try expectTokens(
        \\"hello" "world\n"
    , &.{
        .StringLiteral,
        .StringLiteral,
    });
}

test "Lexer - Keywords and Identifiers" {
    try expectTokens(
        \\fn let mut my_var Return return static
    , &.{
        .Fn,
        .Let,
        .Mut, // mut
        .Identifier, // my_var
        .Identifier, // Return (大写)
        .Return,
        .Static, // static
    });
}

test "Lexer - Comments" {
    const code =
        \\let a = 1; // single line
        \\/* multi 
        \\   line */
        \\let b = 2;
        \\/* nested /* inside */ outside */
        \\let c = 3;
    ;

    try expectTokens(code, &.{
        .Let, .Identifier, .Assign, .IntLiteral, .Semicolon, // let a = 1;
        .Let, .Identifier, .Assign, .IntLiteral, .Semicolon, // let b = 2;
        .Let, .Identifier, .Assign, .IntLiteral, .Semicolon, // let c = 3;
    });
}

test "Lexer - Span Correctness" {
    const code = "let a";
    var lex = Lexer.init(code);

    const t1 = lex.next(); // let
    try std.testing.expectEqual(TokenType.Let, t1.tag);
    try std.testing.expectEqual(0, t1.span.start);
    try std.testing.expectEqual(3, t1.span.end);

    const t2 = lex.next(); // a
    try std.testing.expectEqual(TokenType.Identifier, t2.tag);
    try std.testing.expectEqual(4, t2.span.start); // 注意中间有个空格
    try std.testing.expectEqual(5, t2.span.end);
}

test "Lexer - Slice Content" {
    const code = "var foo = 123;";
    var lex = Lexer.init(code);

    _ = lex.next(); // var

    const t_ident = lex.next(); // foo
    // 验证 Span 切出来的字符串是不是 "foo"
    try std.testing.expect(std.mem.eql(u8, "foo", code[t_ident.span.start..t_ident.span.end]));
}
