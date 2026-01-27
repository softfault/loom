const std = @import("std");
const assert = std.debug.assert;

pub const Span = struct {
    start: usize,
    end: usize,

    pub fn new(start: usize, end: usize) Span {
        assert(start <= end);
        return .{
            .start = start,
            .end = end,
        };
    }

    pub fn len(self: Span) usize {
        return self.end - self.start;
    }

    pub fn merge(self: Span, other: Span) Span {
        return .{
            .start = @min(self.start, other.start),
            .end = @max(self.end, other.end),
        };
    }

    /// 从源码中切出对应的字符串
    pub fn slice(self: Span, source: []const u8) []const u8 {
        assert(self.end <= source.len);
        return source[self.start..self.end];
    }
};
