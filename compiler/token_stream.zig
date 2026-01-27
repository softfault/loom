const std = @import("std");
const Token = @import("token.zig").Token;
const TokenType = @import("token.zig").TokenType;
const Lexer = @import("lexer.zig").Lexer;
const Span = @import("utils.zig").Span;

/// 一个带缓冲的 Token 流，支持有限的 Lookahead (默认 4 个)
pub const TokenStream = struct {
    lexer: Lexer,
    buffer: [4]Token = undefined,
    buffered_count: usize = 0,
    // 记录上一个被消费的 Token 的位置
    // 初始化为一个空 Span (比如 0..0)
    prev_token_span: Span = .{ .start = 0, .end = 0 },

    pub fn init(lexer: Lexer) TokenStream {
        return .{
            .lexer = lexer,
        };
    }

    /// 查看第 N 个 Token (不消耗)
    /// peek(0) 是当前 Token
    /// peek(1) 是下一个
    pub fn peek(self: *TokenStream, n: usize) Token {
        if (n >= 4) @panic("Lookahead limit exceeded! Max is 3.");

        // 如果需要的 Token 不在缓冲区，就去 Lexer 拉取
        while (self.buffered_count <= n) {
            self.buffer[self.buffered_count] = self.lexer.next();
            self.buffered_count += 1;
        }

        return self.buffer[n];
    }

    /// 消耗并返回当前的 Token
    pub fn advance(self: *TokenStream) Token {
        var t: Token = undefined;

        if (self.buffered_count > 0) {
            t = self.buffer[0];
            self.buffered_count -= 1;
            if (self.buffered_count > 0) {
                std.mem.copyForwards(Token, self.buffer[0..self.buffered_count], self.buffer[1 .. self.buffered_count + 1]);
            }
        } else {
            t = self.lexer.next();
        }

        self.prev_token_span = t.span;
        return t;
    }

    /// 辅助方法：消耗当前 Token，并断言它是预期的类型
    pub fn consume(self: *TokenStream, expected: TokenType) !Token {
        const t = self.peek(0);
        if (t.tag == expected) {
            return self.advance();
        }
        // TODO: 返回更详细的错误信息
        return error.UnexpectedToken;
    }
};
