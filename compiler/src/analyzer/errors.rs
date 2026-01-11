use crate::source::FileId;
use crate::utils::Span;
use std::fmt;

/// 包含完整上下文的语义错误
#[derive(Debug, Clone)]
pub struct SemanticError {
    /// 错误发生的具体文件 ID
    pub file_id: FileId,
    /// 错误发生的文本范围
    pub span: Span,
    /// 具体的错误类型（包含结构化数据）
    pub kind: SemanticErrorKind,
}

/// 语义错误的具体变体
/// 这里根据 Loom 的特性预设了一些常见错误
#[derive(Debug, Clone)]
pub enum SemanticErrorKind {
    /// 存 String 而不是 Symbol，方便直接打印
    UndefinedSymbol(String),

    TypeMismatch {
        expected: String,
        found: String,
    },

    ArgumentCountMismatch {
        func_name: String, // 存 String
        expected: usize,
        found: usize,
    },

    DuplicateDefinition(String), // 存 String

    Custom(String),

    /// 模块文件未找到 (路径)
    ModuleNotFound(String),

    /// 模块路径非法 (路径)
    InvalidModulePath(String),

    /// 循环依赖 (路径)
    CircularDependency(String),

    /// 文件读取/加载失败 (原因)
    FileIOError(String),

    /// 模块解析失败 (错误信息)
    ModuleParseError(String),

    /// 循环继承 (涉及的 Table 名)
    CyclicInheritance(String),

    /// 继承目标非法 (比如继承了 int 或不存在的类型)
    /// 参数：具体的类型描述
    InvalidParentType(String),

    /// 泛型参数数量不匹配
    /// (Table 名, 期望数量, 实际数量)
    GenericArgumentCountMismatch {
        name: String,
        expected: usize,
        found: usize,
    },

    /// 字段类型不匹配 (字段名, 期望类型, 实际类型)
    FieldTypeMismatch {
        field: String,
        expected: String,
        found: String,
    },

    /// 抽象方法未实现 (Table 名, 方法名)
    MissingAbstractImplementation {
        table: String,
        method: String,
    },

    /// 方法重写不匹配 (方法名, 详情信息)
    MethodOverrideMismatch {
        method: String,
        reason: String, // 比如 "params count mismatch", "return type mismatch"
    },

    /// 约束违反 (字段名, 详情)
    ConstraintViolation {
        field: String,
        reason: String,
    },

    /// 数组/元组元素类型不一致 (索引, 期望类型, 实际类型)
    ArrayElementTypeMismatch {
        index: usize,
        expected: String,
        found: String,
    },

    /// 一元运算非法 (操作符, 类型)
    InvalidUnaryOperand {
        op: String,
        ty: String,
    },

    /// 二元运算非法 (操作符, 左类型, 右类型)
    InvalidBinaryOperand {
        op: String,
        lhs: String,
        rhs: String,
    },

    /// 赋值目标非法 (原因)
    InvalidAssignmentTarget(String),

    /// 索引非法 (原因, 比如索引不是int)
    InvalidIndexType(String),

    /// 类型不可索引 (类型)
    TypeNotIndexable(String),

    /// 类型不可迭代 (类型)
    TypeNotIterable(String),

    /// If/Else 分支类型不兼容 (Then类型, Else类型)
    IfBranchIncompatible {
        then_ty: String,
        else_ty: String,
    },

    /// If 缺少 Else 分支且返回值非 Unit
    IfMissingElse(String),

    /// 循环条件必须是布尔值
    ConditionNotBool(String), // "If" or "While"

    NotCallable(String),

    ReturnOutsideFunction,

    // [New] 泛型参数遮蔽/重复
    // 比如 [Box<T>] map = <T>... (这里的 T 遮蔽了类的 T)
    GenericShadowing(String),

    // [New] 类型转换非法 (源类型, 目标类型)
    InvalidCast {
        src: String,
        target: String,
    },
}

// === 手动实现 Display，替代 thiserror ===

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 直接打印 kind，以后这里可以扩展成更复杂的格式
        write!(f, "{}", self.kind)
    }
}

impl fmt::Display for SemanticErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // --- 基础符号与定义 ---
            SemanticErrorKind::UndefinedSymbol(name) => {
                write!(f, "Undefined symbol: '{}'", name)
            }
            SemanticErrorKind::DuplicateDefinition(name) => {
                write!(f, "Duplicate definition of '{}'", name)
            }
            SemanticErrorKind::Custom(msg) => {
                write!(f, "{}", msg)
            }

            // --- 类型检查通用 ---
            SemanticErrorKind::TypeMismatch { expected, found } => {
                write!(
                    f,
                    "Type mismatch: expected type '{}', but found '{}'",
                    expected, found
                )
            }

            // --- 函数调用 ---
            SemanticErrorKind::ArgumentCountMismatch {
                func_name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Function '{}' expects {} arguments, but got {}",
                    func_name, expected, found
                )
            }
            SemanticErrorKind::NotCallable(ty) => {
                write!(f, "Type '{}' is not callable", ty)
            }
            SemanticErrorKind::ReturnOutsideFunction => {
                write!(f, "Return statement used outside of a function body")
            }

            // --- 模块系统 ---
            SemanticErrorKind::ModuleNotFound(path) => {
                write!(f, "Module not found: '{}'", path)
            }
            SemanticErrorKind::InvalidModulePath(path) => {
                write!(f, "Invalid module path: '{}'", path)
            }
            SemanticErrorKind::CircularDependency(path) => {
                write!(f, "Circular dependency detected importing '{}'", path)
            }
            SemanticErrorKind::FileIOError(msg) => {
                write!(f, "File I/O error: {}", msg)
            }
            SemanticErrorKind::ModuleParseError(msg) => {
                write!(f, "Module parse error: {}", msg)
            }

            // --- 继承与泛型 ---
            SemanticErrorKind::CyclicInheritance(name) => {
                write!(f, "Cyclic inheritance detected involving '{}'", name)
            }
            SemanticErrorKind::InvalidParentType(ty) => {
                write!(
                    f,
                    "Invalid parent type: '{}'. Parent must be a Table or Generic Instance",
                    ty
                )
            }
            SemanticErrorKind::GenericArgumentCountMismatch {
                name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Generic argument count mismatch for '{}': expected {}, found {}",
                    name, expected, found
                )
            }
            SemanticErrorKind::MissingAbstractImplementation { table, method } => {
                write!(
                    f,
                    "Table '{}' is missing implementation for abstract method '{}'",
                    table, method
                )
            }
            SemanticErrorKind::MethodOverrideMismatch { method, reason } => {
                write!(f, "Method '{}' override mismatch: {}", method, reason)
            }
            SemanticErrorKind::ConstraintViolation { field, reason } => {
                write!(f, "Constraint violation in field '{}': {}", field, reason)
            }

            // --- 结构体与字段 ---
            SemanticErrorKind::FieldTypeMismatch {
                field,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Type mismatch for field '{}': expected '{}', but found '{}'",
                    field, expected, found
                )
            }

            // --- 表达式与运算 ---
            SemanticErrorKind::ArrayElementTypeMismatch {
                index,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Array element at index {} type mismatch: expected '{}', found '{}'",
                    index, expected, found
                )
            }
            SemanticErrorKind::InvalidUnaryOperand { op, ty } => {
                write!(
                    f,
                    "Invalid operand type '{}' for unary operator '{}'",
                    ty, op
                )
            }
            SemanticErrorKind::InvalidBinaryOperand { op, lhs, rhs } => {
                write!(
                    f,
                    "Invalid operand types for binary operator '{}': '{}' and '{}'",
                    op, lhs, rhs
                )
            }
            SemanticErrorKind::InvalidAssignmentTarget(reason) => {
                write!(f, "Invalid assignment target: {}", reason)
            }
            SemanticErrorKind::InvalidIndexType(reason) => {
                write!(f, "Invalid index type: {}", reason)
            }
            SemanticErrorKind::TypeNotIndexable(ty) => {
                write!(f, "Type '{}' is not indexable", ty)
            }
            SemanticErrorKind::TypeNotIterable(ty) => {
                write!(f, "Type '{}' is not iterable", ty)
            }

            // --- 控制流 ---
            SemanticErrorKind::IfBranchIncompatible { then_ty, else_ty } => {
                write!(
                    f,
                    "'if' and 'else' branches have incompatible types: '{}' vs '{}'",
                    then_ty, else_ty
                )
            }
            SemanticErrorKind::IfMissingElse(then_ty) => {
                write!(
                    f,
                    "'if' expression without 'else' branch must evaluate to Unit, but got '{}'",
                    then_ty
                )
            }
            SemanticErrorKind::ConditionNotBool(ctx) => {
                write!(f, "{} condition must be a boolean", ctx)
            }
            SemanticErrorKind::GenericShadowing(name) => {
                write!(
                    f,
                    "Generic parameter '{}' shadows an existing generic parameter from the class or outer scope",
                    name
                )
            }
            SemanticErrorKind::InvalidCast { src, target } => {
                write!(f, "Cannot cast type '{}' to '{}'", src, target)
            }
        }
    }
}
