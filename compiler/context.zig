const std = @import("std");
const Diagnostic = @import("diagnostic.zig").Diagnostic;
const DiagnosticType = @import("diagnostic.zig").DiagnosticType;
const SourceManager = @import("utils.zig").SourceManager;
const StringInterner = @import("utils.zig").StringInterner;
const Span = @import("utils.zig").Span;
const SymbolId = @import("utils.zig").SymbolId;

pub const Context = struct {
    interner: StringInterner,
    source_manager: SourceManager,
    // 诊断列表
    diagnostics: std.ArrayList(Diagnostic),
    // 专门用于诊断系统（存列表内存 + 存消息字符串）
    diag_arena: std.heap.ArenaAllocator,
    // 错误计数器 (用于快速判断编译是否失败)
    error_count: usize = 0,

    pub fn init(allocator: std.mem.Allocator) Context {
        return .{
            .interner = StringInterner.init(allocator),
            .source_manager = SourceManager.init(allocator),
            .diagnostics = .empty,
            .diag_arena = std.heap.ArenaAllocator.init(allocator),
            .error_count = 0,
        };
    }

    pub fn deinit(self: *Context) void {
        self.interner.deinit();
        self.source_manager.deinit();
        self.diagnostics.deinit(self.diag_arena.allocator());
        self.diag_arena.deinit();
        self.* = undefined;
    }

    /// 核心方法：报告错误
    pub fn report(self: *Context, span: Span, tag: DiagnosticType, msg: []const u8) !void {
        // msg 已经在外面分配好了，直接存入
        try self.diagnostics.append(self.diag_arena.allocator(), .{
            .tag = tag,
            .span = span,
            .message = msg,
        });

        if (tag == .Error) {
            self.error_count += 1;
        }
    }

    /// 快捷方法：报告 Error
    pub fn addError(self: *Context, span: Span, fmt: []const u8, args: anytype) !void {
        return self.report(span, .Error, fmt, args);
    }

    /// 判断是否有致命错误
    pub fn hasErrors(self: *Context) bool {
        return self.error_count > 0;
    }

    /// 打印所有错误到 stderr
    pub fn printDiagnostics(self: *Context) !void {
        const stderr = std.io.getStdErr().writer();

        for (self.diagnostics.items) |diag| {
            // 这里调用 SourceManager 来根据 Span 获取文件名和行号
            const loc = self.source_manager.getLocation(diag.span);

            // 简单的格式： file:line:col: Error: message
            try stderr.print("{s}:{}:{}: {s}: {s}\n", .{
                loc.filename,
                loc.line,
                loc.column,
                @tagName(diag.tag),
                diag.message,
            });

            // 进阶：这里可以调用 SourceManager 打印带下划线的源代码片段
            // try self.source_manager.printSnippet(stderr, diag.span);
        }
    }

    pub fn intern(self: *Context, string: []const u8) !SymbolId {
        return self.interner.intern(string);
    }

    pub fn resolve(self: *Context, id: SymbolId) []const u8 {
        return self.interner.resolve(id);
    }
};
