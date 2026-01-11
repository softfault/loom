use crate::utils::Span;
use core::ops::Deref;

/// AST节点ID
///
/// 用于处理AST节点的比较和获取
/// 由`Parser::next_id`生产
/// 并确保唯一性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node<T> {
    pub id: NodeId,
    pub span: Span,
    pub data: T,
}
impl<T> Node<T> {
    pub fn new(id: NodeId, span: Span, data: T) -> Self {
        Self { id, span, data }
    }
}
impl<T> Deref for Node<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}
