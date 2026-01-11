use super::errors::RuntimeErrorKind;
use crate::analyzer::TableId;
use crate::ast::MethodDefinition;
use crate::context::Context;
use crate::interpreter::Environment;
use crate::source::FileId; // [New] 引入 FileId
use crate::utils::{Interner, Symbol}; // [New] 引入 TableId (确保它是 pub 的)

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // === 基础类型 ===
    Nil,
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),

    // === 复杂类型 ===
    Array(Rc<RefCell<Vec<Value>>>),

    // [修改] 模块：现在只存 FileId
    // 解释器通过这个 FileId 去 Driver/Context 里找对应的导出表或 AST
    Module(FileId, Rc<RefCell<Environment>>),

    // [New] 类对象 / Table 类型
    // 当你访问 `my_lib.Config` 时，返回的就是这个值。
    // 它是一个"工厂"，可以被调用 (Call) 来产生 Instance。
    Table(TableId),

    // [修改] Table 实例
    // 内部结构变了，见下文 Instance 定义
    Instance(Rc<Instance>),

    // === 可调用对象 ===
    Function(FileId, Symbol, Rc<RefCell<Environment>>),

    NativeFunction(NativeFunc),

    // [修改] 绑定方法
    // 同样，Instance 内部已经包含了 TableId
    BoundMethod(Rc<Instance>, MethodDefinition, Rc<RefCell<Environment>>),

    BoundNativeMethod(Box<Value>, NativeFunc),

    Range(Box<Value>, Box<Value>),
}

pub type NativeFuncPtr = fn(&mut Context, &[Value]) -> Result<Value, RuntimeErrorKind>;

// 1. 定义包装器
#[derive(Clone)]
pub struct NativeFunc {
    name: String, // 或者用 Symbol，看你喜好。String 对原生函数调试更友好
    func: NativeFuncPtr,
}

// 2. 关键点：自定义 PartialEq
// 我们认为：只要名字一样，就是同一个原生函数。
// 这样既避开了比较指针的 warning，又符合人类直觉。
impl PartialEq for NativeFunc {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

// 3. 自定义 Debug
// 打印出来是 <native fn print> 而不是 <native fn>，调试极其舒服
impl fmt::Debug for NativeFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<native fn {}>", self.name)
    }
}

impl NativeFunc {
    // 提供构造函数
    pub fn new(name: &str, func: NativeFuncPtr) -> Self {
        Self {
            name: name.to_string(),
            func,
        }
    }

    // === 魔法在这里 ===
    // 定义一个 call 方法转发调用
    pub fn call(&self, ctx: &mut Context, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
        (self.func)(ctx, args)
    }
}

// [修改] Table 实例结构
#[derive(Debug, PartialEq)]
pub struct Instance {
    // [修改] 使用 TableId 而不是 Symbol
    // 这样我们才能区分不同文件里的同名类 (比如 lib.Config 和 main.Config)
    pub table_id: TableId,

    pub fields: RefCell<HashMap<Symbol, Value>>,
}

// ModuleEnv 结构体可以删除了，我们现在用 FileId + Context 来管理

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn to_string(&self, interner: &Interner) -> String {
        match self {
            Value::Nil => "nil".to_string(),
            Value::Unit => "()".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Str(s) => s.clone(),

            Value::Array(arr) => {
                let borrowed = arr.borrow();
                // 防止无限递归打印 (简单处理)
                if borrowed.len() > 10 {
                    format!("[Array(len={})]", borrowed.len())
                } else {
                    let elements: Vec<String> =
                        borrowed.iter().map(|v| v.to_string(interner)).collect();
                    format!("[{}]", elements.join(", "))
                }
            }

            // [修改] 打印实例
            Value::Instance(inst) => {
                // inst.table_id 是 (FileId, Symbol)
                // 我们只打印 Symbol 部分给用户看
                let name = interner.resolve(inst.table_id.symbol());
                format!("<instance {}>", name)
            }

            // [New] 打印类对象
            Value::Table(id) => {
                let name = interner.resolve(id.symbol());
                format!("<class {}>", name)
            }

            // [修改] 打印模块
            Value::Module(file_id, _) => {
                // 打印时只显示 ID，环境内容太多了不打印
                format!("<module #{:?}>", file_id)
            }

            Value::Function(_file_id, name, ..) => {
                let func_name = interner.resolve(*name);
                format!("<fn {}>", func_name)
            }

            Value::NativeFunction(_) => "<native fn>".to_string(),

            Value::BoundMethod(inst, method, ..) => {
                let class_name = interner.resolve(inst.table_id.symbol());
                let method_name = interner.resolve(method.name);
                format!("<bound method {}.{}>", class_name, method_name)
            }

            Value::BoundNativeMethod(_receiver, _) => {
                // 递归调用 receiver 的 to_string 有死循环风险，简单处理
                "<bound native method>".to_string()
            }

            Value::Range(start, end) => format!("{}..{}", start, end),
        }
    }
}

// Display 实现保持简略即可
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Unit => write!(f, "()"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{}", s),
            Value::Array(_) => write!(f, "[...]"),
            Value::Instance(_inst) => write!(f, "<instance>"), // 简略
            Value::Table(_) => write!(f, "<class>"),
            Value::Module(..) => write!(f, "<module>"),
            Value::Range(start, end) => write!(f, "{}..{}", start, end),
            _ => write!(f, "<...>"),
        }
    }
}
