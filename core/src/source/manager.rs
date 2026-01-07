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

    /// 根据 ID 获取文件的完整路径
    pub fn get_file_path(&self, id: FileId) -> Option<&PathBuf> {
        self.files.get(id.get()).map(|f| &f.path)
    }

    pub fn update_file(&mut self, id: FileId, new_src: String) {
        if let Some(file) = self.files.get_mut(id.get()) {
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
