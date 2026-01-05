use std::{
    fs, io,
    ops::Index,
    path::{Path, PathBuf},
};

use crate::source::{FileId, SourceFile};

#[derive(Debug, Default)]
pub struct SourceManager {
    files: Vec<SourceFile>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn load_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<FileId> {
        let path = path.as_ref();
        let abs_path = fs::canonicalize(path)?;

        if let Some((id, _)) = self
            .files
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == abs_path)
        {
            return Ok(FileId::new(id));
        }

        let src = fs::read_to_string(&abs_path)?;
        // [修改] 不再维护全局 offset
        let file = SourceFile::new(abs_path, src);
        let id = FileId::new(self.files.len());
        self.files.push(file);
        Ok(id)
    }

    pub fn add_file(&mut self, name: String, src: String) -> io::Result<FileId> {
        // [修改] 直接创建
        let file = SourceFile::new(PathBuf::from(name), src);
        let id = FileId::new(self.files.len());
        self.files.push(file);
        Ok(id)
    }

    pub fn get_file(&self, id: FileId) -> &SourceFile {
        &self[id]
    }

    pub fn get_file_name(&self, id: FileId) -> Option<&str> {
        self.files.get(id.get()).map(|f| f.name.as_str())
    }

    /// [核心修改] 现在通过 (FileId, Offset) 来查找，而不是全局 Offset
    pub fn lookup_location(&self, id: FileId, offset: usize) -> Option<(usize, usize, &str)> {
        let file = self.files.get(id.get())?;
        Some(file.lookup_location(offset))
    }

    /// [LSP] 更新文件内容 (用于处理 textDocument/didChange)
    /// 这里的 src 是用户正在编辑的最新代码
    pub fn update_file(&mut self, id: FileId, new_src: String) {
        if let Some(file) = self.files.get_mut(id.get()) {
            // 我们重新创建一个 SourceFile 来重新计算 line_starts
            // 注意：这里路径保持不变
            let new_file = SourceFile::new(file.path.clone(), new_src);
            *file = new_file;
        }
    }
}

impl Index<FileId> for SourceManager {
    type Output = SourceFile;

    fn index(&self, index: FileId) -> &Self::Output {
        &self.files[index.get()]
    }
}
