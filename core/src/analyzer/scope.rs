use super::info::*;
use super::types::Type;
use crate::source::FileId;
use crate::utils::{Span, Symbol};
use std::collections::HashMap;

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
        // [New] 必须传入定义时的位置
        span: Span,
        // [New] 必须传入定义时的文件 (通常是 analyzer.current_file_id)
        file_id: FileId,
        allow_shadow: bool,
    ) -> Result<(), SymbolKind> {
        let current_scope = self.scopes.last_mut().unwrap();

        if !allow_shadow {
            if let Some(existing) = current_scope.symbols.get(&name) {
                return Err(existing.kind.clone());
            }
        }

        current_scope.symbols.insert(
            name,
            SymbolInfo {
                name,
                ty,
                kind,
                defined_span: span,    // 存下来！
                defined_file: file_id, // 存下来！
            },
        );
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
