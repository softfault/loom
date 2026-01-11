#![allow(unused)]
use std::path::PathBuf;

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub name: String,
    pub src: String,
    pub line_starts: Vec<usize>,
}

impl SourceFile {
    // [修改] new 不再需要传入 start_offset
    pub fn new(path: PathBuf, src: String) -> Self {
        // 计算每一行的起始位置
        let line_starts = std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let name = path.to_string_lossy().to_string();

        Self {
            path,
            name,
            src,
            line_starts,
        }
    }

    pub fn lookup_line(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line + 1,
            Err(line) => line,
        }
    }

    /// 返回 (行号, 列号, 该行文本内容)
    /// offset 是文件内的局部偏移量
    pub fn lookup_location(&self, offset: usize) -> (usize, usize, &str) {
        let line_num = self.lookup_line(offset);
        // line_starts 是从0开始索引的，但 line_num 是从1开始的
        let line_start = self.line_starts[line_num - 1];
        let col_num = offset - line_start + 1;

        // 获取该行文本用于显示
        let line_end = if line_num < self.line_starts.len() {
            self.line_starts[line_num] - 1 // -1 去掉换行符
        } else {
            self.src.len()
        };

        // 防御性切片
        let line_text = if line_start <= self.src.len() && line_end <= self.src.len() {
            // 处理空行或最后一行的情况
            if line_start > line_end {
                ""
            } else {
                &self.src[line_start..line_end]
            }
        } else {
            ""
        };

        (line_num, col_num, line_text)
    }

    /// [LSP 必需] 将 (行号, 列号) 转换为字节偏移量
    /// 注意：传入的 line 和 col 这里假设是 0-based (LSP 标准)，
    /// 但 Loom 内部显示用的是 1-based，所以要注意转换。
    pub fn offset_at(&self, line: usize, col: usize) -> Option<usize> {
        // line_starts 存储的是每一行的起始 offset
        // 如果 line 超出总行数，返回 None
        if line >= self.line_starts.len() {
            return None;
        }

        let line_start = self.line_starts[line];

        // 计算下一行的起始位置，用来确定当前行的长度
        let line_end = if line + 1 < self.line_starts.len() {
            self.line_starts[line + 1]
        } else {
            self.src.len() + 1 // +1 是为了包容最后一行可能的 EOF
        };

        let target_offset = line_start + col;

        // 简单的边界检查，防止列号超出当前行长度
        if target_offset >= line_end {
            // 或者你可以选择返回该行末尾
            return None;
        }

        Some(target_offset)
    }
}
