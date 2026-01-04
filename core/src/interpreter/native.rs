// src/interpreter/native.rs

use super::value::Value;
use crate::context::Context;

// [修改] print 现在可以完美打印对象名了！
pub fn native_print(ctx: &mut Context, args: &[Value]) -> Value {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        // [关键] 调用 arg.to_string(&ctx.interner)
        print!("{}", arg.to_string(&ctx.interner));
    }
    println!();
    Value::Unit
}

// 其他函数也要更新签名，即使它们不用 ctx
pub fn native_str_len(_ctx: &mut Context, args: &[Value]) -> Value {
    if let Some(Value::Str(s)) = args.get(0) {
        Value::Int(s.len() as i64)
    } else {
        Value::Nil
    }
}

pub fn native_array_len(_ctx: &mut Context, args: &[Value]) -> Value {
    if let Some(Value::Array(arr)) = args.get(0) {
        Value::Int(arr.borrow().len() as i64)
    } else {
        Value::Nil
    }
}

pub fn native_array_push(_ctx: &mut Context, args: &[Value]) -> Value {
    if args.len() < 2 {
        return Value::Nil;
    }
    if let Value::Array(arr) = &args[0] {
        arr.borrow_mut().push(args[1].clone());
        Value::Unit
    } else {
        Value::Nil
    }
}
