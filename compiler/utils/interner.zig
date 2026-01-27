const std = @import("std");
const assert = std.debug.assert;

/// 唯一的缓存ID
pub const SymbolId = enum(u32) {
    _,
    pub const INVALID = std.math.maxInt(u32);
};

pub const StringInterner = struct {
    map: std.StringHashMapUnmanaged(SymbolId),
    list: std.ArrayListUnmanaged([]const u8),
    arena: std.heap.ArenaAllocator,

    /// `map`字段和list字段使用`intern`和`resolve`方法使用，而不是直接交互
    /// 这里其实是`undef`
    pub fn init(child_allocator: std.mem.Allocator) StringInterner {
        return .{
            .arena = std.heap.ArenaAllocator.init(child_allocator),
            .map = .{},
            .list = .{},
        };
    }

    /// 只需要deinit内部的arena即可，所有内存都和内部arena有关
    /// 其他的直接undefined即可，因为在栈上。(都是`Unmanaged`)
    pub fn deinit(self: *StringInterner) void {
        self.arena.deinit();
        self.* = undefined;
    }

    /// 分配一个缓存ID
    /// 可能的报错：
    /// 1. 来自`allocator`
    /// 2. 来自`list`
    /// 3. 来自`map`
    /// 都是`Allocator.Error`。
    /// TODO: 具体错误分析
    pub fn intern(self: *StringInterner, string: []const u8) !SymbolId {
        const allocator = self.arena.allocator();

        if (self.map.get(string)) |id| {
            return id;
        }

        const dupe_string = try allocator.dupe(u8, string);
        const index = self.list.items.len; // index 是 usize
        try self.list.append(allocator, dupe_string);

        const id_u32 = @as(u32, @intCast(index));
        const id = @as(SymbolId, @enumFromInt(id_u32));

        try self.map.put(allocator, dupe_string, id);
        return id;
    }

    pub fn resolve(self: *StringInterner, id: SymbolId) []const u8 {
        const index = @intFromEnum(id);

        assert(index < self.list.items.len);
        return self.list.items[index];
    }
};
