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
            match item {
                // 1. 检查类定义 (包含类的方法)
                TopLevelItem::Table(def) => {
                    self.check_table_definition(def);
                }

                // 2. [New] 检查顶层函数
                TopLevelItem::Function(func_def) => {
                    // 传入 None，表示没有父类 Table
                    self.check_function_like_body(func_def, None);
                }

                // 3. [New] 检查顶层变量
                TopLevelItem::Field(field_def) => {
                    self.check_top_level_field(field_def);
                }

                // Use 语句在 Collect 阶段已经处理完了，Check 阶段不需要管
                TopLevelItem::Use(_) => {}
            }
        }
    }
}
