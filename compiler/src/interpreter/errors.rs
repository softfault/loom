#[derive(Debug, Clone)]
pub enum RuntimeErrorKind {
    /// 变量未定义 (Lexical Scope)
    UndefinedVariable(String),

    /// 尝试调用不可调用的对象
    NotCallable(String),

    /// 类型错误: 期望 String, 实际是 Int
    TypeError {
        expected: String,
        found: String,
    },

    /// 参数数量错误
    ArgumentCountMismatch {
        func_name: String,
        expected: usize,
        found: usize,
    },

    /// 索引越界
    IndexOutOfBounds {
        index: i64,
        len: usize,
    },

    /// 属性不存在 (Member Access)
    PropertyNotFound {
        target_type: String,
        property: String,
    },

    /// 除零错误
    DivisionByZero,

    /// 用户自定义错误 (比如 panic("msg"))
    Custom(String),

    /// 内部错误 (VM Bug)
    Internal(String),

    InvalidCast {
        src: String,
        target: String,
    },
}

impl std::fmt::Display for RuntimeErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeErrorKind::UndefinedVariable(name) => {
                write!(f, "Reference Error: variable '{}' is not defined", name)
            }
            RuntimeErrorKind::NotCallable(val_str) => {
                write!(f, "Type Error: '{}' is not callable", val_str)
            }
            RuntimeErrorKind::TypeError { expected, found } => {
                write!(f, "Type Error: expected {}, got {}", expected, found)
            }
            RuntimeErrorKind::ArgumentCountMismatch {
                func_name,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Argument Error: '{}' expects {} arguments, got {}",
                    func_name, expected, found
                )
            }
            RuntimeErrorKind::IndexOutOfBounds { index, len } => {
                write!(
                    f,
                    "Index Error: index {} out of bounds (len {})",
                    index, len
                )
            }
            RuntimeErrorKind::PropertyNotFound {
                target_type,
                property,
            } => {
                write!(
                    f,
                    "Property Error: property '{}' not found on {}",
                    property, target_type
                )
            }
            RuntimeErrorKind::DivisionByZero => write!(f, "Math Error: division by zero"),
            RuntimeErrorKind::Custom(msg) => write!(f, "Error: {}", msg),
            RuntimeErrorKind::Internal(msg) => write!(f, "Internal VM Error: {}", msg),
            RuntimeErrorKind::InvalidCast { src, target } => {
                write!(f, "Cast Error: cannot cast type '{}' to '{}'", src, target)
            }
        }
    }
}
