mod check;
mod collect;
mod errors;
mod path;
mod resolve;
mod scope;
mod tableid;
mod types;

pub use errors::{SemanticError, SemanticErrorKind};
pub use path::resolve_module_path;
pub use scope::{ScopeManager, SymbolKind};
pub use tableid::TableId;
pub use types::{FunctionSignature, ModuleInfo, TableInfo, Type};

use crate::ast::Program;
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
    pub tables: HashMap<Symbol, TableInfo>,

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

        // [Fix] 这里的 params 是 Vec<Type>，不要传参数名 (Symbol)
        // print 函数：接受一个 Any 类型的参数 (或者变长参数如果不方便表示，就先放一个 Any)
        let print_type = Type::Function {
            params: vec![Type::Any],
            ret: Box::new(Type::Unit),
        };

        // 这里的 kind 是 SymbolKind::Function (或者是 Variable，看你定义)
        // allow_shadow = false (内置函数通常不允许重复定义，或者 true 允许用户覆盖)
        let _ = self.scopes.define(
            print_sym,
            print_type,
            SymbolKind::Method, // 假设你有 Function 这个 Kind，或者用 Variable
            false,              // 不允许 Shadowing
        );
    }

    /// 查找 Table 定义
    /// 优先在当前文件找，如果找不到，去所有已加载的模块里找 (处理导入的类)
    fn find_table_info(&self, id: TableId) -> Option<&TableInfo> {
        let TableId(file_id, sym) = id;

        // 1. 如果 file_id 就是当前文件，直接查 self.tables
        if file_id == self.current_file_id {
            return self.tables.get(&sym);
        }

        // 2. 如果是其他文件，去 ModuleInfo 里查
        // 我们需要反查 file_id 对应的 ModuleInfo
        // (性能优化：Context 可以加一个 file_id -> ModuleInfo 的索引，这里先遍历)
        for module in self.ctx.modules.values() {
            if module.file_id == file_id {
                return module.exports.get(&sym);
            }
        }
        None
    }
}
