use super::types::Type;
use crate::utils::Symbol;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: Symbol,
    pub ty: Type,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,  // 本地变量 (var/let)
    Parameter, // 函数参数
    Field,     // Table 字段
    Method,    // Table 方法
    Table,     // Table 类型名
}

#[derive(Debug, Default)]
pub struct Scope {
    pub symbols: HashMap<Symbol, SymbolInfo>,
}

pub struct ScopeManager {
    pub scopes: Vec<Scope>,
}

impl ScopeManager {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::default()], // 至少有一个全局作用域
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        } else {
            panic!("Cannot exit global scope");
        }
    }

    /// [Changed] 定义符号
    /// allow_shadow: 如果为 true，则允许覆盖当前作用域已有的同名符号 (Shadowing)
    pub fn define(
        &mut self,
        name: Symbol,
        ty: Type,
        kind: SymbolKind,
        allow_shadow: bool,
    ) -> Result<(), SymbolKind> {
        let current_scope = self.scopes.last_mut().unwrap();

        if !allow_shadow {
            if let Some(existing) = current_scope.symbols.get(&name) {
                return Err(existing.kind.clone());
            }
        }

        // Insert 会覆盖旧值，这正是 Shadowing 想要的效果
        current_scope
            .symbols
            .insert(name, SymbolInfo { name, ty, kind });
        Ok(())
    }

    /// 查找符号 (从内向外)
    pub fn resolve(&self, name: Symbol) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.symbols.get(&name) {
                return Some(info);
            }
        }
        None
    }

    /// 专门用于查找当前作用域 (例如防止同作用域重复定义)
    pub fn resolve_current(&self, name: Symbol) -> Option<&SymbolInfo> {
        self.scopes.last().unwrap().symbols.get(&name)
    }
}
