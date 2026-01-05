use super::types::{FunctionSignature, Type};
use crate::ast::TableDefinition;
use crate::source::FileId;
use crate::utils::{Span, Symbol};
use std::collections::HashMap;
use std::rc::Rc;

// [New] 字段元数据：不仅包含类型，还包含定义位置
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub ty: Type,
    /// 字段定义在源码中的位置 (用于 Goto Definition)
    pub span: Span,
}

// [New] 方法元数据
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub signature: FunctionSignature,
    /// 方法名定义在源码中的位置
    pub span: Span,
    // 未来如果支持 default implementation，可能还需要 store body AST
}

/// Table 的元数据信息
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: Symbol,
    pub file_id: FileId,
    pub parent: Option<Type>,
    pub generic_params: Vec<Symbol>,

    // [Changed] 从 HashMap<Symbol, Type> 变成 HashMap<Symbol, FieldInfo>
    pub fields: HashMap<Symbol, FieldInfo>,

    // [Changed] 从 HashMap<Symbol, FunctionSignature> 变成 HashMap<Symbol, MethodInfo>
    pub methods: HashMap<Symbol, MethodInfo>,

    pub defined_span: Span,
}

// ModuleInfo 保持不变，它只持有 TableInfo，
// 只要 TableInfo 变强了，ModuleInfo 自动受益。
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub file_id: FileId,
    pub file_path: std::path::PathBuf,
    pub exports: HashMap<Symbol, TableInfo>,
    pub ast_definitions: HashMap<Symbol, Rc<TableDefinition>>,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: Symbol,
    pub ty: Type,
    pub kind: SymbolKind,
    /// [新增] 定义该符号的源码位置
    pub defined_span: Span,
    /// [新增] 定义该符号的文件 (因为可能跳转到另一个文件)
    pub defined_file: FileId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,  // 本地变量 (var/let)
    Parameter, // 函数参数
    Field,     // Table 字段
    Method,    // Table 方法
    Table,     // Table 类型名
}
