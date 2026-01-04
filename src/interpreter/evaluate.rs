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

            // [Fix] 这里不再是 Hack，而是返回真正的 EvalResult::Return
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

            _ => EvalResult::Err(format!("Runtime: Unsupported expression {:?}", expr.data)),
        }
    }

    // ==========================================
    //            Section 1: Basic
    // ==========================================

    fn eval_identifier(&mut self, sym: Symbol) -> EvalResult {
        match self.environment.borrow().get(sym) {
            Some(val) => EvalResult::Ok(val),
            None => EvalResult::Err(format!(
                "Runtime Error: Undefined variable '{}'",
                self.ctx.resolve_symbol(sym)
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
            // 普通原生函数 (print)
            Value::NativeFunction(f) => EvalResult::Ok(f(self.ctx, &arg_values)),

            // [New] 绑定原生方法 (str.len, arr.push)
            Value::BoundNativeMethod(receiver, f) => {
                // 核心逻辑：把 receiver 插入到参数列表的最前面 (self)
                let mut full_args = Vec::with_capacity(arg_values.len() + 1);
                full_args.push(*receiver); // 解包 Box，把 Value 拿出来
                full_args.extend(arg_values); // 接上用户传的参数

                EvalResult::Ok(f(self.ctx, &full_args))
            }

            // 用户自定义方法
            Value::BoundMethod(instance, method_def) => {
                self.call_user_method(instance, &method_def, &arg_values)
            }

            // === Case D: [Fix] 构造函数调用 ===
            // 也就是处理 `l = my_lib.Lib()` 这种情况
            Value::Table(table_id) => {
                // 1. 查找 Table 定义
                // Driver 已经注入了所有的定义，所以这里一定能找到 (除非 ID 错乱)
                let def = match self.table_definitions.get(&table_id).cloned() {
                    Some(d) => d,
                    None => {
                        return EvalResult::Err(format!(
                            "Runtime Error: Definition for class '{}' not found",
                            self.ctx.resolve_symbol(table_id.symbol())
                        ));
                    }
                };

                // 2. 实例化
                // 直接复用 instantiate_table 方法
                // 它会处理字段初始化，并返回 Value::Instance
                let instance_result = self.instantiate_table(&def, table_id.file_id());

                // TODO: 未来可以在这里查找并自动调用 'init' 方法 (构造函数逻辑)

                instance_result
            }

            _ => EvalResult::Err(format!("Trying to call a non-function: {:?}", func)),
        }
    }
    fn call_user_method(
        &mut self,
        receiver: Rc<Instance>,
        method: &MethodDefinition,
        args: &[Value],
    ) -> EvalResult {
        let mut env = Environment::with_enclosing(self.globals.clone());

        env.define(self.ctx.intern("self"), Value::Instance(receiver));

        if args.len() != method.params.len() {
            return EvalResult::Err(format!(
                "Expected {} args, got {}",
                method.params.len(),
                args.len()
            ));
        }
        for (i, param) in method.params.iter().enumerate() {
            env.define(param.name, args[i].clone());
        }

        let prev = self.environment.clone();
        self.environment = Rc::new(RefCell::new(env));

        // 执行方法体
        let result = if let Some(body) = &method.body {
            self.execute_block(body)
        } else {
            EvalResult::Err("Cannot call abstract method".into())
        };

        // 恢复环境
        self.environment = prev;

        // [重要] 这里的行为由 mod.rs 的 call_method 决定是否捕获 Return。
        // 但如果在 evaluate 内部调用 (比如 callee 算出来是 BoundMethod)，
        // 我们需要决定：这里是捕获 Return 还是继续冒泡？
        // 既然这是 "调用一个函数"，那么函数的 Return 对调用者来说就是 Ok(value)。
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
            return EvalResult::Err("Compound assignment not implemented yet".into());
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
                    EvalResult::Err("Cannot assign property on non-instance".into())
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
                        EvalResult::Err("Index out of bounds".into())
                    }
                } else {
                    EvalResult::Err("Invalid index assignment".into())
                }
            }

            _ => EvalResult::Err("Invalid assignment target".into()),
        }
    }

    // ==========================================
    //          Section 4: Control Flow
    // ==========================================

    /// 执行块：这是处理 Return, Scope, Break, Continue 的核心
    pub(super) fn execute_block(&mut self, block: &Block) -> EvalResult {
        // 1. 进入新作用域 (Push Scope)
        let prev_env = self.environment.clone();
        self.environment = Rc::new(RefCell::new(Environment::with_enclosing(prev_env.clone())));

        let mut last_val = Value::Unit;

        for stmt in &block.statements {
            let result = self.evaluate(stmt);
            match result {
                // 正常执行：继续下一条语句
                EvalResult::Ok(v) => {
                    last_val = v;
                }

                // [关键修改] 控制流信号 (Return, Break, Continue, Err)
                // 遇到这些信号时，必须：
                // 1. 立即停止当前块的执行
                // 2. 恢复环境 (Pop Scope)
                // 3. 将信号向上冒泡 (Bubble up) 给调用者 (比如 eval_for 或 eval_function)
                other_result => {
                    self.environment = prev_env;
                    return other_result;
                }
            }
        }

        // 2. 正常结束，退出作用域 (Pop Scope)
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
            // 1. 计算条件
            let cond_val = require_ok!(self.evaluate(condition));

            // 判断真假
            let is_true = match cond_val {
                Value::Bool(b) => b,
                _ => return EvalResult::Err("While condition must be a boolean".into()),
            };

            if !is_true {
                break;
            }

            // 2. 执行循环体 (While 也可以有自己的作用域，通常建议有)
            let prev_env = self.environment.clone();
            let loop_env = Environment::with_enclosing(prev_env.clone());
            self.environment = Rc::new(RefCell::new(loop_env));

            let result = self.execute_block(body);

            self.environment = prev_env;

            // 3. 处理控制流
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
        // 1. 计算可迭代对象
        let collection_val = require_ok!(self.evaluate(iterable_expr));

        match collection_val {
            Value::Array(arr_rc) => {
                let elements = arr_rc.borrow().clone();

                for item in elements {
                    // 2. 每次循环创建一个新作用域
                    let prev_env = self.environment.clone();
                    let mut loop_env = Environment::with_enclosing(prev_env.clone());

                    // 3. 定义迭代变量 (i)
                    loop_env.define(iterator_sym, item);

                    // 切换环境
                    self.environment = Rc::new(RefCell::new(loop_env));

                    // 4. 执行循环体
                    let result = self.execute_block(body);

                    // 恢复环境
                    self.environment = prev_env;

                    // 5. 处理控制流
                    match result {
                        EvalResult::Ok(_) => continue,    // 正常结束，继续下一次
                        EvalResult::Continue => continue, // 遇到 continue，继续下一次
                        EvalResult::Break => break,       // 遇到 break，跳出循环
                        EvalResult::Return(v) => return EvalResult::Return(v), // return 要直接跳出函数
                        EvalResult::Err(e) => return EvalResult::Err(e),
                    }
                }
                EvalResult::Ok(Value::Unit)
            }
            // 将来可以支持 Range (0..10) 或 String 遍历
            _ => EvalResult::Err(format!("Type {:?} is not iterable", collection_val)),
        }
    }

    // [New] 处理 return 语句
    fn eval_return(&mut self, val_opt: &Option<Box<Expression>>) -> EvalResult {
        let val = if let Some(expr) = val_opt {
            require_ok!(self.evaluate(expr))
        } else {
            Value::Unit
        };
        // 发出 Return 信号
        EvalResult::Return(val)
    }

    // ==========================================
    //          Section 5: Helpers
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
        // === 1. 逻辑运算符 (需要短路求值) ===
        // 注意：我们不能先 evaluate(right)，必须根据 left 的结果决定
        match op {
            BinaryOp::And => {
                let left_val = require_ok!(self.evaluate(left));
                // 如果左边是假，直接返回左边的值 (短路)，不再计算右边
                if !self.is_truthy(&left_val) {
                    return EvalResult::Ok(left_val);
                }
                // 否则返回右边的计算结果
                return self.evaluate(right);
            }
            BinaryOp::Or => {
                let left_val = require_ok!(self.evaluate(left));
                // 如果左边是真，直接返回左边的值 (短路)
                if self.is_truthy(&left_val) {
                    return EvalResult::Ok(left_val);
                }
                return self.evaluate(right);
            }
            _ => {} // 其他运算符继续向下执行
        }

        // === 2. 贪婪求值 (Eager Evaluation) ===
        // 对于算术和比较，我们需要左右两边的值
        let l = require_ok!(self.evaluate(left));
        let r = require_ok!(self.evaluate(right));

        let res = match op {
            // --- 算术运算 ---
            BinaryOp::Add => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                (Value::Str(a), Value::Str(b)) => Value::Str(a + &b),
                (Value::Str(a), other) => Value::Str(format!("{}{}", a, other)), // 允许 "a" + 1
                (other, Value::Str(b)) => Value::Str(format!("{}{}", other, b)), // 允许 1 + "a"
                _ => return EvalResult::Err("Type mismatch for +".into()),
            },

            BinaryOp::Sub => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
                _ => return EvalResult::Err("Type mismatch for -".into()),
            },

            BinaryOp::Mul => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
                (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
                _ => return EvalResult::Err("Type mismatch for *".into()),
            },

            BinaryOp::Div => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return EvalResult::Err("Division by zero".into());
                    }
                    Value::Int(a / b)
                }
                (Value::Float(a), Value::Float(b)) => Value::Float(a / b),
                _ => return EvalResult::Err("Type mismatch for /".into()),
            },

            BinaryOp::Mod => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return EvalResult::Err("Modulo by zero".into());
                    }
                    Value::Int(a % b)
                }
                (Value::Float(a), Value::Float(b)) => Value::Float(a % b),
                _ => return EvalResult::Err("Type mismatch for %".into()),
            },

            // --- 比较运算 ---

            // 相等性检查 (利用 Value 的 PartialEq)
            BinaryOp::Eq => Value::Bool(l == r),
            BinaryOp::Neq => Value::Bool(l != r),

            // 大小比较
            BinaryOp::Lt => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a < b),
                // 允许 Int 和 Float 比较 (稍微灵活一点)
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) < b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a < (b as f64)),
                _ => return EvalResult::Err("Invalid types for <".into()),
            },

            BinaryOp::Lte => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a <= b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) <= b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a <= (b as f64)),
                _ => return EvalResult::Err("Invalid types for <=".into()),
            },

            BinaryOp::Gt => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a > b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) > b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a > (b as f64)),
                _ => return EvalResult::Err("Invalid types for >".into()),
            },

            BinaryOp::Gte => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
                (Value::Float(a), Value::Float(b)) => Value::Bool(a >= b),
                (Value::Int(a), Value::Float(b)) => Value::Bool((a as f64) >= b),
                (Value::Float(a), Value::Int(b)) => Value::Bool(a >= (b as f64)),
                _ => return EvalResult::Err("Invalid types for >=".into()),
            },

            // 逻辑运算已经在上面处理过了，这里应该不会走到
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
                _ => return EvalResult::Err("Invalid type for negation".into()),
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
                    EvalResult::Err("Index out of bounds".into())
                }
            }
            _ => EvalResult::Err("Index not supported".into()),
        }
    }
}
