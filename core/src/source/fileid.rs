use std::fmt;

#[repr(transparent)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(usize);

impl FileId {
    pub const fn new(id: usize) -> Self {
        Self(id)
    }

    pub const fn get(&self) -> usize {
        self.0
    }

    pub const MAX: Self = Self(usize::MAX);
}

/// let id: FileId = 1.into();
impl From<usize> for FileId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

/// let idx: usize = id.into();
impl From<FileId> for usize {
    fn from(id: FileId) -> Self {
        id.0
    }
}

/// print like normal number
impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
