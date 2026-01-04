// src/interpreter/environment.rs
use super::Value;
use crate::utils::Symbol;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    // 当前作用域的变量
    pub values: HashMap<Symbol, Value>,
    // 外层作用域 (闭包/全局)
    pub enclosing: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            enclosing: None,
        }
    }

    pub fn with_enclosing(enclosing: Rc<RefCell<Environment>>) -> Self {
        Self {
            values: HashMap::new(),
            enclosing: Some(enclosing),
        }
    }

    /// 定义变量 (var a = 1)
    pub fn define(&mut self, name: Symbol, value: Value) {
        self.values.insert(name, value);
    }

    /// 获取变量
    pub fn get(&self, name: Symbol) -> Option<Value> {
        if let Some(v) = self.values.get(&name) {
            return Some(v.clone());
        }
        // 递归查找外层
        if let Some(enclosing) = &self.enclosing {
            return enclosing.borrow().get(name);
        }
        None
    }

    /// 赋值 (a = 2)
    /// 返回 true 表示赋值成功，false 表示变量未定义
    pub fn assign(&mut self, name: Symbol, value: Value) -> bool {
        if self.values.contains_key(&name) {
            self.values.insert(name, value);
            return true;
        }
        if let Some(enclosing) = &self.enclosing {
            return enclosing.borrow_mut().assign(name, value);
        }
        false
    }
}
