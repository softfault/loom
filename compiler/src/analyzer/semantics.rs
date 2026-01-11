use super::types::Type;
use crate::utils::{NodeId, Span, Symbol};
use std::collections::HashMap;

#[allow(unused)]
/// 语义数据库：存储 Analyzer 分析出的所有信息
#[derive(Debug, Default)]
pub struct SemanticDB {
    /// [最关键] 记录每个表达式节点的推导类型
    /// Key: AST 节点的唯一 ID (NodeId)
    /// Value: 推导出的类型
    pub type_map: HashMap<NodeId, Type>,

    /// [最关键] 记录每个"引用"节点指向的"定义"位置
    /// 用于 Goto Definition
    /// Key: 使用处的 NodeId (比如变量名 `x` 的 NodeId)
    /// Value: 定义处的 Span (比如 `x: int` 的 Span)
    pub def_map: HashMap<NodeId, Span>,

    /// [可选] 记录每个 Symbol 的文档注释 (用于 Hover)
    pub docs: HashMap<Symbol, String>,
}

#[allow(unused)]
impl SemanticDB {
    pub fn record_type(&mut self, id: NodeId, ty: Type) {
        self.type_map.insert(id, ty);
    }

    pub fn record_def(&mut self, usage_id: NodeId, def_span: Span) {
        self.def_map.insert(usage_id, def_span);
    }
}
