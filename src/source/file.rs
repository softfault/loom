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
}
