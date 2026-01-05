// crate::context.rs 或 crate::analyzer::db.rs

use super::types::Type;
use crate::ast::NodeId;
use crate::source::FileId;
use crate::utils::Span;
use std::collections::HashMap;

/// 语义数据库：Analyzer 的"副产品"，LSP 的"核心资产"
#[derive(Debug, Default)]
pub struct SemanticDB {
    /// AST 节点 -> 类型
    /// 用于 Hover (显示类型) 和 Dot Access (解析成员)
    pub type_map: HashMap<NodeId, Type>,

    /// AST 引用节点 -> 定义位置 (FileId, Span)
    /// 用于 Goto Definition
    pub def_map: HashMap<NodeId, Location>,
    // 全局符号表其实已经分散在 context.modules 和 analyzer.tables 里了
    // 如果需要统一查询，可以考虑在这里加索引，或者 LSP 直接查 context.modules
}

#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub file_id: FileId,
    pub span: Span,
}
