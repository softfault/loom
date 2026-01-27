const std = @import("std");
const Span = @import("utils.zig").Span;

pub const DiagnosticType = enum {
    Error,
    Warning,
    Note,
};

pub const Diagnostic = struct {
    tag: DiagnosticType,
    span: Span,
    message: []const u8, // 格式化后的消息字符串

    // TODO: 关联的子诊断（例如 "变量在此处定义" 的 Note）
    // related: ?*Diagnostic = null,
};
