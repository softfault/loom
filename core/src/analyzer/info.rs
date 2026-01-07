use super::types::{FunctionSignature, Type};
use crate::analyzer::TableId;
use crate::ast::{MethodDefinition, TableDefinition};
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
    pub generic_params: Vec<Symbol>,
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

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub file_id: FileId,
    pub file_path: std::path::PathBuf,

    // [Changed] 原来只有 exports (TableInfo)，现在需要更多
    // 我们可以把 exports 拆分，或者把 TableInfo, FunctionInfo 都统一成 ExportItem
    pub tables: HashMap<TableId, TableInfo>,
    pub functions: HashMap<Symbol, FunctionInfo>, // [New]
    pub globals: HashMap<Symbol, GlobalVarInfo>,  // [New]

    // 用于 AST 缓存 (如果需要保留给 Interpreter 用)
    pub ast_definitions: HashMap<Symbol, Rc<TableDefinition>>,
    // 还需要缓存顶层函数的 AST 吗？ Interpreter 可能需要。
    pub ast_functions: HashMap<Symbol, Rc<MethodDefinition>>,
    pub program: Rc<crate::ast::Program>,
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
    Function,
}

// [New] 顶层函数元数据
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: Symbol,
    pub generic_params: Vec<Symbol>, // 函数级泛型 <T>
    pub signature: FunctionSignature,
    pub span: Span,
    pub file_id: FileId,
}

// [New] 顶层/全局变量元数据
// 其实和 FieldInfo 很像，但为了语义区分，定义一个新的
#[derive(Debug, Clone)]
pub struct GlobalVarInfo {
    pub name: Symbol,
    pub ty: Type,
    pub span: Span,
    pub file_id: FileId,
    pub is_const: bool, // 未来扩展
}
