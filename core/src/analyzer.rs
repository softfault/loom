mod check;
mod collect;
mod db;
mod errors;
mod info;
mod path;
mod resolve;
mod scope;
mod semantics;
mod tableid;
mod types;

pub use db::{Location, SemanticDB};
pub use errors::{SemanticError, SemanticErrorKind};
pub use info::{FieldInfo, MethodInfo, ModuleInfo, SymbolInfo, TableInfo};
pub use path::resolve_module_path;
pub use scope::ScopeManager;
pub use tableid::TableId;
pub use types::{FunctionSignature, Type};

use crate::analyzer::info::{FunctionInfo, GlobalVarInfo, SymbolKind};
use crate::ast::{NodeId, Program};
use crate::context::Context;
use crate::source::FileId;
use crate::utils::{Span, Symbol};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Analyzer<'a> {
    pub ctx: &'a mut Context,
    pub scopes: ScopeManager,

    /// 存储所有 Table 的元数据 (Loom 核心数据结构)
    /// Key: Table Name
    pub tables: HashMap<TableId, TableInfo>,
    pub functions: HashMap<Symbol, FunctionInfo>,
    pub globals: HashMap<Symbol, GlobalVarInfo>,

    /// 收集到的错误 (非致命)
    pub errors: Vec<SemanticError>,

    // [New] 当前正在检查的函数的期望返回类型
    // 进入 method 时设置，退出时恢复
    pub current_return_type: Option<Type>,

    pub current_file_path: PathBuf,
    pub current_file_id: FileId,
}

impl<'a> Analyzer<'a> {
    pub fn new(ctx: &'a mut Context, file_id: FileId) -> Self {
        // 如果需要 path 用于模块解析，从 ctx 里查出来
        let current_file_path = ctx.source_manager.get_file(file_id).path.clone();

        Self {
            ctx,
            scopes: ScopeManager::new(),
            tables: Default::default(),
            functions: HashMap::new(),
            globals: HashMap::new(),
            errors: Vec::new(),
            current_return_type: None,
            current_file_id: file_id, // 直接存
            current_file_path,        // 从 ID 反查
        }
    }

    pub fn analyze(&mut self, program: &Program) {
        // Step 1: 收集定义 (Collect Definitions)
        // 这一步只看名字和签名，不看具体逻辑，建立 Table 索引
        self.collect_program(program);

        // Step 2: 解析继承与填充 (Resolve Hierarchy)
        // 处理 [Production: Base] 的拷贝逻辑
        self.resolve_hierarchy();

        // Step 3: 类型检查 (Type Check)
        // 深入函数体，检查 x = 1 是否合法，self.host 是否存在
        self.check_program(program);
    }

    /// 记录错误 helper
    pub fn report(&mut self, span: Span, kind: SemanticErrorKind) {
        self.errors.push(SemanticError {
            file_id: self.current_file_id,
            span,
            kind,
        });
    }

    /// 注册内置函数到当前作用域（全局作用域）
    fn register_builtins(&mut self) {
        let print_sym = self.ctx.intern("print");

        let print_type = Type::Function {
            params: vec![Type::Any],
            ret: Box::new(Type::Unit),
        };

        // [Fix] 构造“空”位置信息
        // 1. 使用 Span::default() (即 0..0)
        let dummy_span = crate::utils::Span::default();

        let _ = self.scopes.define(
            print_sym,
            print_type,
            SymbolKind::Method, // 或 SymbolKind::Function
            dummy_span,         // <--- 这里的 Span 是空的
            FileId::BUILTIN,    // <--- 这里的文件 ID 是特殊的
            false,              // 不允许覆盖
        );
    }

    /// 查找 Table 定义
    fn find_table_info(&self, id: TableId) -> Option<TableInfo> {
        // 1. 尝试直接在本地 tables 里找
        if let Some(info) = self.tables.get(&id) {
            return Some(info.clone());
        }

        // 2. 如果没找到，去 ctx.modules 查
        if let Some(path) = self.ctx.source_manager.get_file_path(id.file_id()) {
            if let Some(module) = self.ctx.modules.get(path) {
                // [Fix] 直接用 id 查
                return module.tables.get(&id).cloned();
            }
        }

        None
    }

    /// [LSP Helper] 记录一个表达式的类型
    pub fn record_type(&mut self, node_id: NodeId, ty: Type) {
        self.ctx.db.type_map.insert(node_id, ty);
    }

    /// [LSP Helper] 记录一个引用的定义位置
    pub fn record_def(&mut self, usage_id: NodeId, def_file: FileId, def_span: Span) {
        self.ctx.db.def_map.insert(
            usage_id,
            Location {
                file_id: def_file,
                span: def_span,
            },
        );
    }
}
