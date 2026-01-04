use crate::source::FileId;
use crate::utils::Symbol;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub FileId, pub Symbol);

impl TableId {
    pub fn file_id(&self) -> FileId {
        self.0
    }

    pub fn symbol(&self) -> Symbol {
        self.1
    }
}
