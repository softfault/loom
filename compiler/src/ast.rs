#![allow(unused)]
mod node;

use crate::token::TokenKind;
use crate::utils::{Interner, Span, Symbol};
pub use node::{Node, NodeId};

// --- 顶级结构 ---

// 顶层既可以是 Table 定义，也可以是 Use 语句
#[derive(Debug, Clone)]
pub enum TopLevelItem {
    Table(TableDefinition),
    Function(MethodDefinition),
    Field(FieldDefinition),
    Use(UseStatement),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseAnchor {
    /// 以标识符开头 (std.fs), 表示从 Root 或 Lib 查找
    Root,
    /// 以 . 开头 (.utils), 表示从当前目录查找
    Current,
    /// 以 .. 开头 (..config), 表示从父目录查找
    Parent,
}

#[derive(Debug, Clone)]
pub struct UseStatementData {
    pub anchor: UseAnchor,
    pub path: Vec<Symbol>, // 存储路径片段 ["std", "fs"]
    pub alias: Option<Symbol>,
}
pub type UseStatement = Node<UseStatementData>;

#[derive(Debug, Clone)]
pub struct Program {
    pub definitions: Vec<TopLevelItem>, // 改这里
    pub span: Span,
}

/// Table 定义: [Name: Prototype]
#[derive(Debug, Clone)]
pub struct TableDefinitionData {
    pub name: Symbol,
    /// 继承/约束的原型 (例如 [Production: BaseServer] 中的 BaseServer)
    /// 如果没有原型，则是 None (例如 [BaseServer])
    pub prototype: Option<TypeRef>,
    /// 泛型参数 <T: Constraint>
    pub generics: Vec<GenericParam>,
    /// 表内的条目 (字段或方法)
    pub items: Vec<TableItem>,
}
pub type TableDefinition = Node<TableDefinitionData>;

/// 泛型参数定义 <T: Base>
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParamData {
    pub name: Symbol,
    pub constraint: Option<TypeRef>,
}
pub type GenericParam = Node<GenericParamData>;

/// Table 内部的条目
#[derive(Debug, Clone)]
pub enum TableItem {
    Field(FieldDefinition),
    Method(MethodDefinition),
}

/// 字段定义: host = "localhost" 或 port: int = 8080
#[derive(Debug, Clone)]
pub struct FieldDefinitionData {
    pub name: Symbol,
    /// 显式类型标注 (可选)
    pub type_annotation: Option<TypeRef>,
    /// 默认值/初始值 (必选，除非是纯接口定义？Spec里似乎总是有值的)
    /// 如果允许纯声明 field: int，则 value 为 Option
    pub value: Option<Expression>,
}
pub type FieldDefinition = Node<FieldDefinitionData>;

/// 方法定义: connect = () bool ...
#[derive(Debug, Clone, PartialEq)]
pub struct MethodDefinitionData {
    pub name: Symbol,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>, // 如果推导则为 None，但 Spec 建议显式
    pub body: Option<Block>,          // 方法体
}
pub type MethodDefinition = Node<MethodDefinitionData>;

/// 函数参数: (x: int)
#[derive(Debug, Clone, PartialEq)]
pub struct ParamData {
    pub name: Symbol,
    pub type_annotation: TypeRef,
}
pub type Param = Node<ParamData>;

// --- 类型系统 ---

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRefData {
    /// 基础类型: int, bool, str
    Named(Symbol),
    /// 泛型实例化: List<int>
    GenericInstance {
        base: Symbol,
        args: Vec<TypeRef>,
    },
    /// 结构化类型: { name: str }
    Structural(Vec<Param>),
    // 模块成员类型: std.io.File, lib.Animal
    // 视为一种特殊的引用
    Member {
        module: Symbol, // e.g. "animal_lib"
        member: Symbol, // e.g. "Animal"
    },
    // 数组类型
    // 对应语法: [int], [[str]]
    Array(Box<TypeRef>),
}
pub type TypeRef = Node<TypeRefData>;

// --- 语句与表达式 (一切皆表达式) ---

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionData {
    // --- 字面量 ---
    Literal(Literal),

    // --- 变量/访问 ---
    /// 变量名 identifier
    Identifier(Symbol),
    /// 成员访问 self.factor 或 user.name
    FieldAccess {
        target: Box<Expression>,
        field: Symbol,
    },

    // --- 运算 ---
    /// 二元运算 a + b
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// 一元运算 !flag, -num
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
    },

    // --- 控制流 ---
    /// if cond then_block else else_block
    If {
        condition: Box<Expression>,
        then_block: Block,
        else_block: Option<Block>, // else 是可选的
    },
    /// for i in 0..10
    For {
        iterator: Symbol,
        iterable: Box<Expression>, // 例如 range 0..10
        body: Block,
    },
    /// range 0..10
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
        inclusive: bool, // .. vs ..= (虽然 Spec 目前只用了 ..)
    },

    /// 数组字面量: [1, 2, 3]
    Array(Vec<Expression>),

    Tuple(Vec<Expression>),

    /// 索引访问: arr[index]
    /// 注意：index 可以是一个 Range 表达式 (0..10)，从而支持切片
    Index {
        target: Box<Expression>, // 被索引的对象 (arr)
        index: Box<Expression>,  // 索引值 (i 或 0..5)
    },

    // --- 调用 ---
    /// 统一的调用表达式
    /// 涵盖：
    /// 1. 普通函数调用: add(1, 2)
    /// 2. 泛型调用: run_task<Config>(c)
    /// 3. 原型实例化: Debug(target: "arm64")
    Call {
        callee: Box<Expression>,
        /// 泛型参数，例如 <T, U>。如果没有则是空 Vec
        /// 解决了 <> 问题后，这就只是 AST 里的一个字段
        generic_args: Vec<TypeRef>,
        /// 参数列表 (支持位置参数和命名参数)
        args: Vec<CallArg>,
    },

    /// 实例化 (构造 Table) Debug(target: "arm64")
    /// 这其实也是一种 Call，但在 AST 层面可能需要区分，或者在语义分析时区分
    /// 这里暂时复用 Call，或者定义一个 ConstructorCall
    // --- 块 ---
    Block(Block),

    // --- 显式返回 ---
    Return(Option<Box<Expression>>),
    Break {
        value: Option<Box<Expression>>,
    },
    Continue,

    While {
        condition: Box<Expression>,
        body: Block,
    },

    VariableDefinition {
        is_mut: bool,
        name: Symbol,
        ty: Option<TypeRef>,
        init: Box<Expression>,
    },

    // [New] 赋值表达式
    // 涵盖: =, +=, -=, *=, /=
    Assign {
        op: AssignOp,
        target: Box<Expression>, // 左值 (l-value)
        value: Box<Expression>,  // 右值 (r-value)
    },

    // [New] 类型转换: expr as Type
    Cast {
        expr: Box<Expression>,
        target_type: TypeRef,
    },
}
pub type Expression = Node<ExpressionData>;

// 赋值操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,      // =
    PlusAssign,  // +=
    MinusAssign, // -=
    MulAssign,   // *=
    DivAssign,   // /=
    ModAssign,   // %=
}

/// 代码块 (缩进块)
#[derive(Debug, Clone, PartialEq)]
pub struct BlockData {
    pub statements: Vec<Expression>, // 因为一切皆表达式
}
pub type Block = Node<BlockData>;

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod, // Arithmetic
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte, // Comparison
    And,
    Or, // Logical
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, // -
    Not, // !
}

/// 调用参数
/// Loom 支持混合参数： func(10, width: 20)
#[derive(Debug, Clone, PartialEq)]
pub struct CallArgData {
    /// 参数名 (Optional)。
    /// 如果是位置参数 (10)，则是 None。
    /// 如果是命名参数 (width: 20)，则是 Some("width")。
    pub name: Option<Symbol>,
    pub value: Expression,
}
pub type CallArg = Node<CallArgData>;
