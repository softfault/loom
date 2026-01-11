// core/src/interpreter/native.rs

use super::errors::RuntimeErrorKind;
use super::value::Value;
use crate::context::Context;

// === 辅助 Helper 函数 (让 Native 代码更干净) ===

/// 检查参数数量
fn check_arg_count(
    func_name: &str,
    args: &[Value],
    expected: usize,
) -> Result<(), RuntimeErrorKind> {
    if args.len() != expected {
        return Err(RuntimeErrorKind::ArgumentCountMismatch {
            func_name: func_name.to_string(),
            expected,
            found: args.len(),
        });
    }
    Ok(())
}

/// 强制获取第 N 个参数为 Array
fn expect_array(
    args: &[Value],
    index: usize,
) -> Result<&std::cell::RefCell<Vec<Value>>, RuntimeErrorKind> {
    match args.get(index) {
        Some(Value::Array(arr)) => Ok(arr),
        Some(other) => Err(RuntimeErrorKind::TypeError {
            expected: "Array".into(),
            found: other_type_name(other),
        }),
        None => Err(RuntimeErrorKind::Internal(
            "Missing argument in expect_array".into(),
        )),
    }
}

/// 强制获取第 N 个参数为 String
fn expect_string(args: &[Value], index: usize) -> Result<&String, RuntimeErrorKind> {
    match args.get(index) {
        Some(Value::Str(s)) => Ok(s),
        Some(other) => Err(RuntimeErrorKind::TypeError {
            expected: "String".into(),
            found: other_type_name(other),
        }),
        None => Err(RuntimeErrorKind::Internal(
            "Missing argument in expect_string".into(),
        )),
    }
}

/// 获取类型的显示名称 (用于报错)
fn other_type_name(v: &Value) -> String {
    match v {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "str",
        Value::Array(_) => "Array",
        Value::Instance(_i) => "Instance", // 这里虽然没办法拿 Interner，但这是 Native 层的简略报错
        _ => "unknown",
    }
    .to_string()
}

// === Native 实现 ===

pub fn native_print(ctx: &mut Context, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg.to_string(&ctx.interner));
    }
    println!();
    Ok(Value::Unit)
}

pub fn native_str_len(_ctx: &mut Context, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
    // 自动检查参数数量
    check_arg_count("len", args, 1)?;
    // 自动检查并提取 String
    let s = expect_string(args, 0)?;

    Ok(Value::Int(s.len() as i64))
}

pub fn native_array_len(_ctx: &mut Context, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
    check_arg_count("len", args, 1)?;
    let arr_cell = expect_array(args, 0)?;

    let len = arr_cell.borrow().len() as i64;
    Ok(Value::Int(len))
}

pub fn native_array_push(_ctx: &mut Context, args: &[Value]) -> Result<Value, RuntimeErrorKind> {
    check_arg_count("push", args, 2)?;
    let arr_cell = expect_array(args, 0)?;

    let item = args[1].clone();
    arr_cell.borrow_mut().push(item);

    Ok(Value::Unit)
}
