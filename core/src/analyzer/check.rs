mod call;
mod decl;
mod expr;
mod helpers;
mod stmt;

use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::{Analyzer, SymbolKind, TableInfo, Type};
use crate::ast::*;
use crate::utils::Symbol;
use std::collections::HashMap;

impl<'a> Analyzer<'a> {
    /// Pass 3 入口：类型检查与约束验证
    pub fn check_program(&mut self, program: &Program) {
        for item in &program.definitions {
            if let TopLevelItem::Table(def) = item {
                self.check_table_definition(def);
            }
        }
    }
}
