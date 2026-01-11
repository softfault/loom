/// 一个本地辅助宏，用于简化 EvalResult 的处理
/// 类似于 Rust 的 `?` 操作符，但专门针对 EvalResult
macro_rules! require_ok {
    ($expr:expr) => {
        match $expr {
            EvalResult::Ok(val) => val,
            // 如果是 Return 或 Err，直接向上冒泡，停止当前函数的执行
            other => return other,
        }
    };
}

mod field_access;

use super::environment::Environment;
use super::errors::RuntimeErrorKind;
use super::value::{Instance, Value};
use super::{EvalResult, Interpreter};
use crate::analyzer::TableId;
use crate::ast::*;
use crate::utils::Symbol;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Interpreter<'a> {
    /// === 主入口：表达式求值 ===
    pub fn evaluate(&mut self, expr: &Expression) -> EvalResult {
        match &expr.data {
            // 1. 原子类型
            ExpressionData::Literal(lit) => self.eval_literal(lit),
            ExpressionData::Identifier(sym) => self.eval_identifier(*sym),

            // 2. 访问与调用
            ExpressionData::FieldAccess { target, field } => self.eval_field_access(target, *field),
            ExpressionData::Index { target, index } => self.eval_index(target, index),
            ExpressionData::Call { callee, args, .. } => self.eval_call(callee, args),

            // 3. 赋值
            ExpressionData::Assign { op, target, value } => {
                self.eval_assignment(*op, target, value)
            }

            // 4. 运算
            ExpressionData::Binary { op, left, right } => self.eval_binary(*op, left, right),
            ExpressionData::Unary { op, expr } => self.eval_unary(*op, expr),

            // 5. 控制流
            ExpressionData::Block(blk) => self.execute_block(blk),
            ExpressionData::If {
                condition,
                then_block,
                else_block,
            } => self.eval_if(condition, then_block, else_block),
            ExpressionData::While { condition, body } => self.eval_while(condition, body),

            // 6. 其他
            ExpressionData::Array(elements) => self.eval_array(elements),

            ExpressionData::Return(val) => self.eval_return(val),

            ExpressionData::VariableDefinition { name, init, .. } => {
                let val = require_ok!(self.evaluate(init));
                self.environment.borrow_mut().define(*name, val);
                EvalResult::Ok(Value::Unit)
            }
            ExpressionData::For {
                iterator,
                iterable,
                body,
            } => self.eval_for(*iterator, iterable, body),
            ExpressionData::Break { .. } => EvalResult::Break,
            ExpressionData::Continue => EvalResult::Continue,

            ExpressionData::Range { start, end, .. } => {
                let start_val = require_ok!(self.evaluate(start));
                let end_val = require_ok!(self.evaluate(end));
                EvalResult::Ok(Value::Range(Box::new(start_val), Box::new(end_val)))
            }
            ExpressionData::Cast { expr, target_type } => self.eval_cast(expr, target_type),

            // [Error] 不支持的表达式
            _ => EvalResult::Err(RuntimeErrorKind::Internal(format!(
                "Unsupported expression type: {:?}",
                expr.data
            ))),
        }
    }

    // ==========================================
    //            Section 1: Basic
    // ==========================================

    fn eval_identifier(&mut self, sym: Symbol) -> EvalResult {
        match self.environment.borrow().get(sym) {
            Some(val) => EvalResult::Ok(val),
            None => EvalResult::Err(RuntimeErrorKind::UndefinedVariable(
                self.ctx.resolve_symbol(sym).to_string(),
            )),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> EvalResult {
        let val = match lit {
            Literal::Int(i) => Value::Int(*i),
            Literal::Float(f) => Value::Float(*f),
            Literal::String(s) => Value::Str(s.clone()),
            Literal::Bool(b) => Value::Bool(*b),
            Literal::Nil => Value::Nil,
            _ => Value::Nil,
        };
        EvalResult::Ok(val)
    }

    // ==========================================
    //          Section 2: Access & Call
    // ==========================================

    fn eval_call(&mut self, callee: &Expression, args: &[CallArg]) -> EvalResult {
        let func = require_ok!(self.evaluate(callee));

        let mut arg_values = Vec::new();
        for arg in args {
            arg_values.push(require_ok!(self.evaluate(&arg.value)));
        }

        match func {
            // [Case A] 原生函数
            Value::NativeFunction(_) => self.call_value(func, &arg_values, None),

            // [Case B] 绑定原生方法 (str.len, arr.push)
            Value::BoundNativeMethod(receiver, f) => {
                // 核心逻辑：把 receiver 插入到参数列表的最前面 (self)
                let mut full_args = Vec::with_capacity(arg_values.len() + 1);
                full_args.push(*receiver);
                full_args.extend(arg_values);

                // 这里的 f 返回 Result<Value, RuntimeErrorKind>
                match f.call(self.ctx, &full_args) {
                    Ok(v) => EvalResult::Ok(v),
                    Err(e) => EvalResult::Err(e),
                }
            }

            // [Case C] 用户自定义方法 (Bound Method)
            Value::BoundMethod(instance, method_def, def_env) => {
                self.call_user_method(instance, &method_def, def_env, &arg_values)
            }

            // [Case D] 顶层函数 (Top-level Function)
            // 严谨版本：Function(FileId, Symbol, Env)
            // 不使用 @，直接匹配解构，然后重组传给 call_value
            Value::Function(file_id, func_name, env) => {
                let func_val = Value::Function(file_id, func_name, env);
                self.call_value(func_val, &arg_values, None)
            }

            // [Case E] 构造函数调用 (Table)
            Value::Table(table_id) => {
                // 1. 查找 Table 定义
                let def = match self.table_definitions.get(&table_id).cloned() {
                    Some(d) => d,
                    None => {
                        return EvalResult::Err(RuntimeErrorKind::Internal(format!(
                            "Definition for class '{}' not found",
                            self.ctx.resolve_symbol(table_id.symbol())
                        )));
                    }
                };

                // 2. 实例化
                self.instantiate_table(&def, table_id.file_id())
            }

            // [Error] 不可调用
            _ => EvalResult::Err(RuntimeErrorKind::NotCallable(
                func.to_string(&self.ctx.interner),
            )),
        }
    }

    fn call_user_method(
        &mut self,
        receiver: Rc<Instance>,
        method: &MethodDefinition,
        def_env: Rc<RefCell<Environment>>, // [New] 传入定义环境
        args: &[Value],
    ) -> EvalResult {
        // 1. 构造局部环境
        // [Key Fix] 父环境是 def_env (定义时的模块环境)
        // 这样方法内部就能访问到它定义所在文件的全局变量了！
        let mut env = Environment::with_enclosing(def_env.clone());

        // 2. 定义 self 和参数
        env.define(self.ctx.intern("self"), Value::Instance(receiver));

        if args.len() != method.params.len() {
            return EvalResult::Err(RuntimeErrorKind::ArgumentCountMismatch {
                func_name: self.ctx.resolve_symbol(method.name).to_string(),
                expected: method.params.len(),
                found: args.len(),
            });
        }
        for (i, param) in method.params.iter().enumerate() {
            env.define(param.name, args[i].clone());
        }

        // 3. 切换上下文 (保存 -> 切换 -> 执行 -> 恢复)
        let prev_env = self.environment.clone();
        let prev_globals = self.globals.clone(); // 保存当前的 globals

        // 切换到方法内部环境
        self.environment = Rc::new(RefCell::new(env));
        // [Key Fix] 切换 globals 为定义该方法的模块环境
        // 这样在方法里再调用其他顶层函数时，也能找到正确的函数
        self.globals = def_env;

        // 4. 执行方法体
        let result = if let Some(body) = &method.body {
            self.execute_block(body)
        } else {
            EvalResult::Err(RuntimeErrorKind::Internal(
                "Cannot call abstract method".into(),
            ))
        };

        // 5. 恢复上下文
        self.environment = prev_env;
        self.globals = prev_globals;

        match result {
            EvalResult::Return(v) => EvalResult::Ok(v),
            other => other,
        }
    }

    // ==========================================
    //          Section 3: Assignment
    // ==========================================

    fn eval_assignment(
        &mut self,
        op: AssignOp,
        target: &Expression,
        value_expr: &Expression,
    ) -> EvalResult {
        let right_val = require_ok!(self.evaluate(value_expr));

        if op != AssignOp::Assign {
            return EvalResult::Err(RuntimeErrorKind::Internal(
                "Compound assignment not implemented yet".into(),
            ));
        }

        match &target.data {
            ExpressionData::Identifier(name) => {
                if self
                    .environment
                    .borrow_mut()
                    .assign(*name, right_val.clone())
                {
                    EvalResult::Ok(Value::Unit)
                } else {
                    // 自动定义
                    self.environment.borrow_mut().define(*name, right_val);
                    EvalResult::Ok(Value::Unit)
                }
            }

            ExpressionData::FieldAccess {
                target: obj_expr,
                field,
            } => {
                let obj_val = require_ok!(self.evaluate(obj_expr));
                if let Value::Instance(instance) = obj_val {
                    instance.fields.borrow_mut().insert(*field, right_val);
                    EvalResult::Ok(Value::Unit)
                } else {
                    EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Instance".into(),
                        found: "Non-Instance".into(), // 可以优化显示
                    })
                }
            }

            ExpressionData::Index {
                target: arr_expr,
                index: idx_expr,
            } => {
                let arr_val = require_ok!(self.evaluate(arr_expr));
                let idx_val = require_ok!(self.evaluate(idx_expr));

                if let (Value::Array(vec_rc), Value::Int(i)) = (arr_val, idx_val) {
                    let mut vec = vec_rc.borrow_mut();
                    if i >= 0 && (i as usize) < vec.len() {
                        vec[i as usize] = right_val;
                        EvalResult::Ok(Value::Unit)
                    } else {
                        EvalResult::Err(RuntimeErrorKind::IndexOutOfBounds {
                            index: i,
                            len: vec.len(),
                        })
                    }
                } else {
                    EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Array and Int".into(),
                        found: "Invalid Types".into(),
                    })
                }
            }

            _ => EvalResult::Err(RuntimeErrorKind::Internal(
                "Invalid assignment target".into(),
            )),
        }
    }

    // ==========================================
    //          Section 4: Control Flow
    // ==========================================

    pub(super) fn execute_block(&mut self, block: &Block) -> EvalResult {
        let prev_env = self.environment.clone();
        self.environment = Rc::new(RefCell::new(Environment::with_enclosing(prev_env.clone())));

        let mut last_val = Value::Unit;

        for stmt in &block.statements {
            let result = self.evaluate(stmt);
            match result {
                EvalResult::Ok(v) => {
                    last_val = v;
                }
                other_result => {
                    self.environment = prev_env;
                    return other_result;
                }
            }
        }

        self.environment = prev_env;
        EvalResult::Ok(last_val)
    }

    fn eval_if(
        &mut self,
        condition: &Expression,
        then_block: &Block,
        else_block: &Option<Block>,
    ) -> EvalResult {
        let cond_val = require_ok!(self.evaluate(condition));

        if self.is_truthy(&cond_val) {
            self.execute_block(then_block)
        } else if let Some(else_blk) = else_block {
            self.execute_block(else_blk)
        } else {
            EvalResult::Ok(Value::Unit)
        }
    }

    fn eval_while(&mut self, condition: &Expression, body: &Block) -> EvalResult {
        loop {
            let cond_val = require_ok!(self.evaluate(condition));

            let is_true = match cond_val {
                Value::Bool(b) => b,
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Bool".into(),
                        found: "Non-Bool".into(),
                    });
                }
            };

            if !is_true {
                break;
            }

            let prev_env = self.environment.clone();
            let loop_env = Environment::with_enclosing(prev_env.clone());
            self.environment = Rc::new(RefCell::new(loop_env));

            let result = self.execute_block(body);

            self.environment = prev_env;

            match result {
                EvalResult::Ok(_) => continue,
                EvalResult::Continue => continue,
                EvalResult::Break => break,
                EvalResult::Return(v) => return EvalResult::Return(v),
                EvalResult::Err(e) => return EvalResult::Err(e),
            }
        }
        EvalResult::Ok(Value::Unit)
    }

    fn eval_for(
        &mut self,
        iterator_sym: Symbol,
        iterable_expr: &Expression,
        body: &Block,
    ) -> EvalResult {
        let collection_val = require_ok!(self.evaluate(iterable_expr));

        match collection_val {
            Value::Array(arr_rc) => {
                let elements = arr_rc.borrow().clone();
                for item in elements {
                    let result = self.eval_loop_body(body, Some((iterator_sym, item)));
                    match result {
                        EvalResult::Ok(_) => continue,
                        EvalResult::Continue => continue,
                        EvalResult::Break => break,
                        _ => return result,
                    }
                }
                EvalResult::Ok(Value::Unit)
            }

            Value::Str(s) => {
                for c in s.chars() {
                    let char_val = Value::Str(c.to_string());
                    let result = self.eval_loop_body(body, Some((iterator_sym, char_val)));
                    match result {
                        EvalResult::Ok(_) => continue,
                        EvalResult::Continue => continue,
                        EvalResult::Break => break,
                        _ => return result,
                    }
                }
                EvalResult::Ok(Value::Unit)
            }

            Value::Range(start, end) => {
                let start_i = match start.as_int() {
                    Some(i) => i,
                    None => {
                        return EvalResult::Err(RuntimeErrorKind::TypeError {
                            expected: "Int".into(),
                            found: "Non-Int".into(),
                        });
                    }
                };
                let end_i = match end.as_int() {
                    Some(i) => i,
                    None => {
                        return EvalResult::Err(RuntimeErrorKind::TypeError {
                            expected: "Int".into(),
                            found: "Non-Int".into(),
                        });
                    }
                };

                for i in start_i..end_i {
                    let int_val = Value::Int(i);
                    let result = self.eval_loop_body(body, Some((iterator_sym, int_val)));
                    match result {
                        EvalResult::Ok(_) => continue,
                        EvalResult::Continue => continue,
                        EvalResult::Break => break,
                        _ => return result,
                    }
                }
                EvalResult::Ok(Value::Unit)
            }

            _ => EvalResult::Err(RuntimeErrorKind::TypeError {
                expected: "Iterable (Array, Str, Range)".into(),
                found: format!("{:?}", collection_val),
            }),
        }
    }

    fn eval_loop_body(
        &mut self,
        body: &Block,
        iterator_sym: Option<(Symbol, Value)>,
    ) -> EvalResult {
        let prev_env = self.environment.clone();
        let mut loop_env = Environment::with_enclosing(prev_env.clone());

        if let Some((sym, val)) = iterator_sym {
            loop_env.define(sym, val);
        }

        self.environment = Rc::new(RefCell::new(loop_env));
        let result = self.execute_block(body);
        self.environment = prev_env;

        result
    }

    fn eval_return(&mut self, val_opt: &Option<Box<Expression>>) -> EvalResult {
        let val = if let Some(expr) = val_opt {
            require_ok!(self.evaluate(expr))
        } else {
            Value::Unit
        };
        EvalResult::Return(val)
    }

    // ==========================================
    //          Section 5: Type Casting
    // ==========================================

    fn eval_cast(&mut self, expr: &Expression, target_type: &TypeRef) -> EvalResult {
        let val = require_ok!(self.evaluate(expr));

        // 如果是 Nil，通常允许转换为任何对象类型的 "空" (但 Loom 暂时没有 Nullable 语法，除了 Any)
        // 简单起见，如果值是 Nil，直接返回 Nil (对应 Option 语义)
        if matches!(val, Value::Nil) {
            return EvalResult::Ok(Value::Nil);
        }

        match &target_type.data {
            TypeRefData::Named(sym) => {
                let type_name = self.ctx.resolve_symbol(*sym);

                match type_name {
                    // --- 基础类型转换 ---
                    "int" => match val {
                        Value::Float(f) => EvalResult::Ok(Value::Int(f as i64)),
                        Value::Int(i) => EvalResult::Ok(Value::Int(i)),
                        Value::Bool(b) => EvalResult::Ok(Value::Int(if b { 1 } else { 0 })),
                        _ => self.runtime_cast_error(val, "int"),
                    },
                    "float" => match val {
                        Value::Int(i) => EvalResult::Ok(Value::Float(i as f64)),
                        Value::Float(f) => EvalResult::Ok(Value::Float(f)),
                        _ => self.runtime_cast_error(val, "float"),
                    },
                    "str" => {
                        // as str: 显式转字符串
                        EvalResult::Ok(Value::Str(val.to_string(&self.ctx.interner)))
                    }
                    "bool" => match val {
                        Value::Bool(b) => EvalResult::Ok(Value::Bool(b)),
                        _ => self.runtime_cast_error(val, "bool"),
                    },

                    // --- 对象类型转换 (RTTI) ---
                    _ => {
                        if let Value::Instance(ref instance) = val {
                            // 执行运行时类型检查
                            // 检查 instance 是否是 target_name 的实例或子类
                            if self.check_instance_of(instance, *sym) {
                                EvalResult::Ok(val)
                            } else {
                                let src_type = self
                                    .ctx
                                    .resolve_symbol(instance.table_id.symbol())
                                    .to_string();
                                EvalResult::Err(RuntimeErrorKind::InvalidCast {
                                    src: src_type,
                                    target: type_name.to_string(),
                                })
                            }
                        } else {
                            // 试图把非 Instance 转换为 Class
                            self.runtime_cast_error(val, type_name)
                        }
                    }
                }
            }
            // 对于泛型、数组等复杂类型，Analyzer 已经保证了结构兼容性
            // 运行时通常直接放行 (No-op)
            _ => EvalResult::Ok(val),
        }
    }

    /// 运行时类型检查 (Runtime Type Identification)
    fn check_instance_of(&self, instance: &Instance, target_sym: Symbol) -> bool {
        let mut current_id = instance.table_id;

        loop {
            // 比较 Symbol
            if current_id.symbol() == target_sym {
                return true;
            }

            // 向上查找
            if let Some(parent_id) = self.get_parent_table_id(current_id) {
                current_id = parent_id;
            } else {
                break;
            }
        }
        false
    }

    fn runtime_cast_error(&self, val: Value, target: &str) -> EvalResult {
        EvalResult::Err(RuntimeErrorKind::InvalidCast {
            src: val.to_string(&self.ctx.interner),
            target: target.to_string(),
        })
    }

    // ==========================================
    //          Section 6: Helpers
    // ==========================================

    fn is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            _ => true,
        }
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &Expression, right: &Expression) -> EvalResult {
        match op {
            BinaryOp::And => {
                let left_val = require_ok!(self.evaluate(left));
                if !self.is_truthy(&left_val) {
                    return EvalResult::Ok(left_val);
                }
                return self.evaluate(right);
            }
            BinaryOp::Or => {
                let left_val = require_ok!(self.evaluate(left));
                if self.is_truthy(&left_val) {
                    return EvalResult::Ok(left_val);
                }
                return self.evaluate(right);
            }
            _ => {}
        }

        let l = require_ok!(self.evaluate(left));
        let r = require_ok!(self.evaluate(right));

        let res = match op {
            BinaryOp::Add => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                (Value::Str(a), Value::Str(b)) => Value::Str(a + &b),
                (Value::Str(a), other) => Value::Str(format!("{}{}", a, other)),
                (other, Value::Str(b)) => Value::Str(format!("{}{}", other, b)),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Addable".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Sub => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Number".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Mul => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Number".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Div => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return EvalResult::Err(RuntimeErrorKind::DivisionByZero);
                    }
                    Value::Int(a / b)
                }
                (Value::Float(a), Value::Float(b)) => Value::Float(a / b),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Number".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Mod => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return EvalResult::Err(RuntimeErrorKind::DivisionByZero);
                    }
                    Value::Int(a % b)
                }
                (Value::Float(a), Value::Float(b)) => Value::Float(a % b),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Number".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Eq => Value::Bool(l == r),
            BinaryOp::Neq => Value::Bool(l != r),

            BinaryOp::Lt => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a < b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) < b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a < (b as f64)),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Comparable".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Lte => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a <= b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) <= b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a <= (b as f64)),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Comparable".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Gt => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a > b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) > b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a > (b as f64)),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Comparable".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            BinaryOp::Gte => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a >= b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) >= b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a >= (b as f64)),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Comparable".into(),
                        found: "Mismatch".into(),
                    });
                }
            },

            _ => Value::Unit,
        };

        EvalResult::Ok(res)
    }

    fn eval_unary(&mut self, op: UnaryOp, expr: &Expression) -> EvalResult {
        let val = require_ok!(self.evaluate(expr));
        let res = match op {
            UnaryOp::Not => Value::Bool(!self.is_truthy(&val)),
            UnaryOp::Neg => match val {
                Value::Int(i) => Value::Int(-i),
                Value::Float(f) => Value::Float(-f),
                _ => {
                    return EvalResult::Err(RuntimeErrorKind::TypeError {
                        expected: "Number".into(),
                        found: "Mismatch".into(),
                    });
                }
            },
        };
        EvalResult::Ok(res)
    }

    fn eval_array(&mut self, elements: &[Expression]) -> EvalResult {
        let mut vals = Vec::new();
        for e in elements {
            vals.push(require_ok!(self.evaluate(e)));
        }
        EvalResult::Ok(Value::Array(Rc::new(RefCell::new(vals))))
    }

    fn eval_index(&mut self, target: &Expression, index: &Expression) -> EvalResult {
        let t_val = require_ok!(self.evaluate(target));
        let i_val = require_ok!(self.evaluate(index));

        match (t_val, i_val) {
            (Value::Array(arr), Value::Int(idx)) => {
                let vec = arr.borrow();
                if idx >= 0 && (idx as usize) < vec.len() {
                    EvalResult::Ok(vec[idx as usize].clone())
                } else {
                    EvalResult::Err(RuntimeErrorKind::IndexOutOfBounds {
                        index: idx,
                        len: vec.len(),
                    })
                }
            }
            _ => EvalResult::Err(RuntimeErrorKind::TypeError {
                expected: "Indexable".into(),
                found: "Mismatch".into(),
            }),
        }
    }
}
