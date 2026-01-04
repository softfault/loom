// context.rs
use crate::analyzer::ModuleInfo;
use crate::source::SourceManager;
use crate::utils::{Interner, Symbol};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Context {
    pub interner: Interner,
    pub source_manager: SourceManager,
    // [New] 项目根目录 (用于解析 Root anchor)
    pub root_dir: PathBuf,

    /// [Global Cache] 已加载的模块
    /// Key: 文件的绝对路径 (Canonical Path)
    /// Value: 模块的语义信息 (导出了什么 Table)
    pub modules: HashMap<PathBuf, ModuleInfo>,

    /// [Cycle Detection] 正在加载中的文件路径
    /// 用于检测循环依赖 (A -> B -> A)
    pub loading_stack: HashSet<PathBuf>,
}

impl Context {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            interner: Interner::new(),
            source_manager: SourceManager::new(),
            root_dir: root_dir.canonicalize().unwrap_or(root_dir),
            modules: HashMap::new(),
            loading_stack: HashSet::new(),
        }
    }

    pub fn intern(&mut self, name: &str) -> Symbol {
        self.interner.intern(name)
    }

    pub fn resolve_symbol(&self, sym: Symbol) -> &str {
        self.interner.resolve(sym)
    }
}
