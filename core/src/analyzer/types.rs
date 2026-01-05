use super::tableid::TableId;
use crate::ast::TableDefinition;
use crate::source::FileId;
use crate::utils::{Interner, Span, Symbol};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// 基础类型
    Int,
    Float,
    Bool,
    Str,
    Char,
    Any, // 类似于 TypeScript 的 any，用于未标注或动态情况
    Nil,

    /// 单元类型 (Void)，用于没有返回值的函数
    Unit,

    /// 具名 Table 类型 (例如 "BaseServer")
    /// 在 Loom 中，一个 Table 定义既是对象工厂，也是类型定义
    Table(TableId),

    // [New] 泛型参数占位符
    // 当我们在 [Box<T>] 内部看到 T 时，它不是一个 Table，而是一个 GenericParam
    GenericParam(Symbol),

    /// 泛型实例化 (例如 List<int>)
    /// 这里的 args 已经是解析后的 Type，而不是 AST 的 TypeRef
    GenericInstance {
        base: TableId,
        args: Vec<Type>,
    },

    /// 数组类型 (从泛型或字面量推导)
    Array(Box<Type>),

    /// 函数/方法类型
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },

    /// 结构化类型 (匿名接口) { name: str }
    /// 用 map 存储字段名 -> 类型
    Structural(Vec<(Symbol, Type)>),

    // --- 内部状态 ---
    /// 待推导类型 (用户没写类型标注，如 `x = 10`)
    /// Analyzer 稍后会将其更新为具体类型
    Infer,

    /// 错误类型 (用于错误恢复，避免级联报错)
    Error,

    // [New] 元组类型
    Tuple(Vec<Type>),

    // [New] 范围类型 (用于 0..10)
    // Range<Int>
    Range(Box<Type>),

    Never,
    Module(PathBuf),
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub params: Vec<(Symbol, Type)>,
    pub ret: Type,
    pub is_abstract: bool,
}

impl Type {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// 简单的类型兼容性检查 (Assignability)
    /// check if `other` can be assigned to `self`
    pub fn is_assignable_from(&self, other: &Type) -> bool {
        // 1. Never 可以赋值给任何类型 (Bottom Type)
        //    例如: let x: int = return; 是合法的
        if *other == Type::Never {
            return true;
        }

        // 2. 任何类型都不能赋值给 Never (除了 Never 自己)
        //    不能把 int 赋值给 !，因为 ! 意味着不返回值
        if *self == Type::Never {
            return false;
        }

        if *self == Type::Any
            || *other == Type::Any
            || *self == Type::Error
            || *other == Type::Error
        {
            return true;
        }

        match (self, other) {
            (Type::Int, Type::Int) => true,
            (Type::Float, Type::Float) => true,
            // 允许 Int 自动提升为 Float? Loom 偏向强类型，暂不允许隐式转换，除非显式 cast
            (Type::Bool, Type::Bool) => true,
            (Type::Str, Type::Str) => true,
            (Type::Unit, Type::Unit) => true,
            (Type::GenericParam(a), Type::GenericParam(b)) => a == b,
            (Type::Table(s1), Type::Table(s2)) => s1 == s2, // 暂时只支持名义类型相等，原型继承兼容性稍后处理
            (Type::Array(t1), Type::Array(t2)) => t1.is_assignable_from(t2),
            (
                Type::Function {
                    params: t_params,
                    ret: t_ret,
                },
                Type::Function {
                    params: s_params,
                    ret: s_ret,
                },
            ) => {
                // 1. 参数数量必须一致
                if t_params.len() != s_params.len() {
                    return false;
                }

                // 2. 参数检查：逆变 (Contravariance)
                // 规则：Source 的参数必须是 Target 参数的父类 (或相同)
                // 即：Source.param.is_assignable_from(Target.param)
                for (i, t_param) in t_params.iter().enumerate() {
                    let s_param = &s_params[i];
                    // 注意这里的顺序！是用 Source 去容纳 Target 的输入
                    if !s_param.is_assignable_from(t_param) {
                        return false;
                    }
                }

                // 3. 返回值检查：协变 (Covariance)
                // 规则：Source 的返回值必须是 Target 返回值的子类 (或相同)
                // 即：Target.ret.is_assignable_from(Source.ret)
                if !t_ret.is_assignable_from(s_ret) {
                    return false;
                }

                true
            }
            // Container<int> 兼容 Container<int>
            (
                Type::GenericInstance { base: b1, args: a1 },
                Type::GenericInstance { base: b2, args: a2 },
            ) => {
                // 1. 必须是同一个基础类 (Container == Container)
                if b1 != b2 {
                    return false;
                }

                // 2. 泛型参数数量必须一致
                if a1.len() != a2.len() {
                    return false;
                }

                // 3. 递归检查每一个泛型参数
                // 这里我们使用 is_assignable_from 递归检查
                // 这意味着 List<Dog> 可以赋值给 List<Animal> (协变)
                for (p1, p2) in a1.iter().zip(a2.iter()) {
                    if !p1.is_assignable_from(p2) {
                        return false;
                    }
                }

                true
            }
            _ => false,
        }
    }

    pub fn to_string(&self, interner: &crate::utils::Interner) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Str => "str".to_string(),
            Type::Char => "char".to_string(),
            Type::Any => "any".to_string(),
            Type::Nil => "nil".to_string(),
            Type::Unit => "()".to_string(),
            Type::Infer => "_".to_string(),
            Type::Error => "<error>".to_string(),
            Type::Never => "!".to_string(),

            Type::Table(sym) => interner.resolve(sym.symbol()).to_string(),

            // 模块类型 (显示简短名字)
            Type::Module(path) => {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                format!("module<{}>", name)
            }

            Type::Array(inner) => format!("[{}]", inner.to_string(interner)),

            Type::GenericParam(sym) => interner.resolve(*sym).to_string(),

            Type::GenericInstance { base, args } => {
                let base_str = interner.resolve(base.symbol());
                let args_str: Vec<_> = args.iter().map(|a| a.to_string(interner)).collect();
                format!("{}<{}>", base_str, args_str.join(", "))
            }

            Type::Tuple(types) => {
                let elements: Vec<_> = types.iter().map(|t| t.to_string(interner)).collect();
                format!("({})", elements.join(", "))
            }

            Type::Range(inner) => format!("Range<{}>", inner.to_string(interner)),

            Type::Function { params, ret } => {
                // 假设 params 是 Vec<(Symbol, Type)>，如果只是 Vec<Type> 则去掉 .1
                let p_str: Vec<_> = params.iter().map(|p| p.to_string(interner)).collect();
                format!("({}) -> {}", p_str.join(", "), ret.to_string(interner))
            }

            Type::Structural(_) => "{ ... }".to_string(),
        }
    }
    /// 类型替换：将泛型参数替换为具体类型
    /// mapping: { "T" => int, "U" => str }
    pub fn substitute(&self, mapping: &HashMap<Symbol, Type>) -> Type {
        match self {
            Type::GenericParam(sym) => {
                // 如果当前类型是 T，且 mapping 里有 T -> int，则替换为 int
                if let Some(concrete_type) = mapping.get(sym) {
                    concrete_type.clone()
                } else {
                    // 如果没找到 (比如是 T，但 mapping 只有 U)，保持原样
                    self.clone()
                }
            }
            Type::GenericInstance { base, args } => {
                // 递归替换参数: List<T> -> List<int>
                let new_args = args.iter().map(|a| a.substitute(mapping)).collect();
                Type::GenericInstance {
                    base: *base,
                    args: new_args,
                }
            }
            Type::Array(inner) => Type::Array(Box::new(inner.substitute(mapping))),
            // Table, Int, Str 等不受泛型影响
            _ => self.clone(),
        }
    }

    // 获取类型名称（用于查找 Table 定义）
    pub fn get_base_symbol(&self) -> Option<Symbol> {
        match self {
            Type::Table(s) => Some(s.symbol()),
            Type::GenericInstance { base, .. } => Some(base.symbol()),
            _ => None,
        }
    }

    pub fn display<'a>(&'a self, interner: &'a Interner) -> TypePrinter<'a> {
        TypePrinter { ty: self, interner }
    }
}

// 2. 定义打印辅助结构体
pub struct TypePrinter<'a> {
    ty: &'a Type,
    interner: &'a Interner,
}

// 3. 为 Printer 实现 Display
impl<'a> fmt::Display for TypePrinter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let interner = self.interner;
        match self.ty {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::Char => write!(f, "char"),
            Type::Any => write!(f, "any"),
            Type::Nil => write!(f, "nil"),
            Type::Unit => write!(f, "()"),
            Type::Infer => write!(f, "_"),
            Type::Error => write!(f, "<error>"),
            Type::Never => write!(f, "!"),

            Type::Table(sym) => write!(f, "{}", interner.resolve(sym.symbol())),
            Type::GenericParam(sym) => write!(f, "{}", interner.resolve(*sym)),

            Type::Module(path) => {
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                write!(f, "module<{}>", name)
            }

            Type::Array(inner) => write!(f, "[{}]", inner.display(interner)),

            Type::GenericInstance { base, args } => {
                let base_str = interner.resolve(base.symbol());
                write!(f, "{}<", base_str)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg.display(interner))?;
                }
                write!(f, ">")
            }

            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t.display(interner))?;
                }
                write!(f, ")")
            }

            Type::Range(inner) => write!(f, "Range<{}>", inner.display(interner)),

            Type::Function { params, ret } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p.display(interner))?;
                }
                write!(f, ") -> {}", ret.display(interner))
            }

            // 改进 Structural 的打印
            Type::Structural(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", interner.resolve(*name), ty.display(interner))?;
                }
                write!(f, " }}")
            }
        }
    }
}
