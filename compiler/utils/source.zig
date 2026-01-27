const std = @import("std");
const Allocator = std.mem.Allocator;

pub const SourceFile = struct {
    allocator: Allocator,
    path: []const u8,
    name: []const u8,
    src: []const u8,
    line_starts: []usize,

    pub fn init(allocator: Allocator, path: []const u8, src: []const u8) !SourceFile {
        const path_dupe = try allocator.dupe(u8, path);
        const src_dupe = try allocator.dupe(u8, src);
        const name_slice = std.fs.path.basename(path_dupe);
        const name_dupe = try allocator.dupe(u8, name_slice);

        // 只有发生错误时才需要释放上面已经分配的内存
        errdefer {
            allocator.free(path_dupe);
            allocator.free(src_dupe);
            allocator.free(name_dupe);
        }

        // 计算行首
        var lines = std.ArrayList(usize).init(allocator);
        errdefer lines.deinit();

        try lines.append(0); // 第一行肯定是从 0 开始

        // 遍历所有字节找换行符
        for (src_dupe, 0..) |byte, i| {
            if (byte == '\n') {
                try lines.append(i + 1);
            }
        }

        return SourceFile{
            .allocator = allocator,
            .path = path_dupe,
            .name = name_dupe,
            .src = src_dupe,
            .line_starts = try lines.toOwnedSlice(), // 把 ArrayList 转换成定长 slice
        };
    }

    pub fn deinit(self: *SourceFile) void {
        self.allocator.free(self.path);
        self.allocator.free(self.name);
        self.allocator.free(self.src);
        self.allocator.free(self.line_starts);
    }

    /// 返回 1-based 行号
    pub fn lookupLine(self: SourceFile, offset: usize) usize {

        // 二分查找
        var low: usize = 0;
        var high: usize = self.line_starts.len;

        // 寻找第一个大于 offset 的元素位置
        while (low < high) {
            const mid = low + (high - low) / 2;
            if (self.line_starts[mid] > offset) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }

        // low 就是 1-based 的行号
        // 如果 offset 在 line 0 的范围内，line_starts[1] > offset，所以返回 1。
        return low;
    }

    pub const Location = struct {
        line: usize,
        column: usize,
        text: []const u8,
    };

    pub fn lookupLocation(self: SourceFile, offset: usize) Location {
        const line_num = self.lookupLine(offset);
        const line_idx = line_num - 1; // 0-based index
        const line_start = self.line_starts[line_idx];

        // 计算列号 (1-based)
        const col_num = offset - line_start + 1;

        // 计算该行结束位置
        var line_end: usize = 0;
        if (line_idx + 1 < self.line_starts.len) {
            line_end = self.line_starts[line_idx + 1] - 1; // -1 去掉换行符
        } else {
            line_end = self.src.len;
        }

        // 防御性切片
        var line_text: []const u8 = "";
        if (line_start <= self.src.len and line_end <= self.src.len) {
            if (line_start <= line_end) {
                line_text = self.src[line_start..line_end];
            }
        }

        return .{
            .line = line_num,
            .col = col_num,
            .text = line_text,
        };
    }

    /// LSP 偏移量转换 (line, col 均为 0-based)
    pub fn offsetAt(self: SourceFile, line: usize, col: usize) ?usize {
        if (line >= self.line_starts.len) return null;

        const start = self.line_starts[line];

        // 计算当前行长度限制
        var end: usize = 0;
        if (line + 1 < self.line_starts.len) {
            end = self.line_starts[line + 1]; // 包含换行符
        } else {
            end = self.src.len + 1; // 允许指到 EOF
        }

        const target_offset = start + col;
        if (target_offset >= end) return null;

        return target_offset;
    }
};

pub const FileId = enum(u32) {
    _, // 允许这是个非穷举的 enum，或者直接用 struct 包装

    pub fn new(id: usize) FileId {
        return @enumFromInt(@as(u32, @intCast(id)));
    }

    pub fn index(self: FileId) usize {
        return @intCast(@intFromEnum(self));
    }
};

pub const SourceManager = struct {
    allocator: Allocator,
    files: std.ArrayList(SourceFile),

    pub fn init(allocator: Allocator) SourceManager {
        return .{
            .allocator = allocator,
            .files = std.ArrayList(SourceFile).empty,
        };
    }

    pub fn deinit(self: *SourceManager) void {
        for (self.files.items) |*file| {
            file.deinit();
        }
        self.files.deinit(self.allocator);
    }

    /// 路径标准化 -> 查重 -> 读取 -> 创建
    pub fn loadFile(self: *SourceManager, path: []const u8) !FileId {
        // 1. 路径标准化
        // realpathAlloc 分配了新内存，必须记得释放
        const abs_path = try std.fs.cwd().realpathAlloc(self.allocator, path);
        defer self.allocator.free(abs_path); // 这里的 abs_path 只是用来查重和读取的临时变量

        // 2. 查重
        for (self.files.items, 0..) |*file, i| {
            if (std.mem.eql(u8, file.path, abs_path)) {
                return FileId.new(i);
            }
        }

        // 3. 读取文件内容
        const file_handle = try std.fs.cwd().openFile(abs_path, .{});
        defer file_handle.close();

        // 限制最大读取大小，防止读爆内存 (这里设置为 1GB)
        const src_content = try file_handle.readToEndAlloc(self.allocator, 1024 * 1024 * 1024);
        defer self.allocator.free(src_content); // SourceFile.init 会做深拷贝，所以这里用完要释放

        // 4. 创建并添加
        // SourceFile.init 内部会复制 path 和 src，所以可以安全释放上面的临时变量
        const file = try SourceFile.init(self.allocator, abs_path, src_content);

        try self.files.append(self.allocator, file);
        return FileId.new(self.files.items.len - 1);
    }

    /// 用于测试或虚拟文件
    pub fn addFile(self: *SourceManager, name: []const u8, src: []const u8) !FileId {
        // 直接根据传入的 name 和 src 创建
        const file = try SourceFile.init(self.allocator, name, src);
        try self.files.append(self.allocator, file);
        return FileId.new(self.files.items.len - 1);
    }

    /// Index trait
    pub fn getFile(self: *SourceManager, id: FileId) *SourceFile {
        return &self.files.items[id.index()];
    }

    pub fn getFileConst(self: SourceManager, id: FileId) SourceFile {
        return self.files.items[id.index()];
    }

    pub fn updateFile(self: *SourceManager, id: FileId, new_src: []const u8) !void {
        const idx = id.index();
        if (idx >= self.files.items.len) return;

        const old_file = &self.files.items[idx];

        // 1. 创建新文件对象 (SourceFile.init 会拷贝 new_src)
        const new_file = try SourceFile.init(self.allocator, old_file.path, new_src);

        // 2. 先释放旧文件的内存
        // 不写这行，旧文件持有的 path/src/line_starts 堆内存会泄漏。
        old_file.deinit();

        // 3. 覆盖
        self.files.items[idx] = new_file;
    }

    /// 核心查找逻辑
    pub fn lookupLocation(self: SourceManager, id: FileId, offset: usize) ?SourceFile.Location {
        if (id.index() >= self.files.items.len) return null;
        const file = self.files.items[id.index()];
        return file.lookupLocation(offset);
    }
};
