use crate::analyzer::TableId;
use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::{Analyzer, FunctionSignature, Type};
use crate::utils::Symbol;
use std::collections::{HashMap, HashSet};

impl<'a> Analyzer<'a> {
    /// Pass 2 入口：解析继承层次与填充
    pub fn resolve_hierarchy(&mut self) {
        // [Key] 使用 TableId 而不是 Symbol
        let mut resolved = HashSet::new();
        let mut visiting = HashSet::new();

        // 获取当前文件的所有 Table ID
        // 注意：self.tables 的 key 已经是 TableId 了 (上一轮 collect 阶段修改的)
        let table_ids: Vec<TableId> = self.tables.keys().cloned().collect();

        for id in table_ids {
            let _ = self.resolve_table(id, &mut resolved, &mut visiting);
        }
    }

    /// 递归解析单个 Table
    fn resolve_table(
        &mut self,
        table_id: TableId,
        resolved: &mut HashSet<TableId>,
        visiting: &mut HashSet<TableId>,
    ) -> Result<(), ()> {
        if resolved.contains(&table_id) {
            return Ok(());
        }

        // 循环检测
        if visiting.contains(&table_id) {
            let span = self.get_table_span(table_id);
            let name_str = self.ctx.resolve_symbol(table_id.symbol()).to_string();
            self.report(span, SemanticErrorKind::CyclicInheritance(name_str));
            return Err(());
        }

        visiting.insert(table_id);

        // 获取父类引用 (Cloned to avoid borrow issues)
        let parent_type_opt = self.tables.get(&table_id).and_then(|t| t.parent.clone());

        if let Some(parent_type) = parent_type_opt {
            // 提取父类的 TableId (无论是本地 Named 还是跨模块 Module.Member，都已经解析为 TableId)
            let parent_id = match &parent_type {
                Type::Table(id) => *id,
                Type::GenericInstance { base, .. } => *base,
                // 如果是 Error，说明之前解析失败了，直接跳过
                Type::Error => {
                    visiting.remove(&table_id);
                    return Err(());
                }
                _ => {
                    // 不支持继承 int/str 等
                    visiting.remove(&table_id);
                    return Err(());
                }
            };

            // 1. 检查父类是否存在 (使用新函数 fetch_table_info)
            if self.fetch_table_info(parent_id).is_none() {
                let span = self.get_table_span(table_id);
                let p_name = self.ctx.resolve_symbol(parent_id.symbol()).to_string();
                self.report(span, SemanticErrorKind::UndefinedSymbol(p_name));
                visiting.remove(&table_id);
                return Err(());
            }

            // 2. 递归解析父类
            // [Critical] 只有当父类也在当前文件时，才需要递归 resolve_table
            // 如果父类在外部文件，说明那个文件已经分析过了 (Status: Resolved)，不需要再跑 resolve_table
            if parent_id.file_id() == self.current_file_id {
                if self.resolve_table(parent_id, resolved, visiting).is_err() {
                    visiting.remove(&table_id);
                    return Err(());
                }
            }

            // 3. 执行填充 (The Static Copy)
            // 无论是本地还是外部，都执行这个！
            if self.fill_from_parent(table_id, &parent_type).is_err() {
                visiting.remove(&table_id);
                return Err(());
            }
        }

        visiting.remove(&table_id);
        resolved.insert(table_id);
        Ok(())
    }

    fn fill_from_parent(&mut self, child_id: TableId, parent_type: &Type) -> Result<(), ()> {
        let parent_id = match parent_type {
            Type::Table(id) => *id,
            Type::GenericInstance { base, .. } => *base,
            _ => return Err(()),
        };

        // 1. [Fix] 获取父类信息 (支持跨文件)
        let parent_info = match self.fetch_table_info(parent_id) {
            Some(info) => info,
            None => return Err(()), // 理论上上面已经 check 过了
        };

        // 2. 泛型映射 (保持不变)
        let mut type_mapping = HashMap::new();
        if let Type::GenericInstance { args, .. } = parent_type {
            if args.len() != parent_info.generic_params.len() {
                let span = self.get_table_span(child_id);
                let p_name = self.ctx.resolve_symbol(parent_id.symbol()).to_string();
                self.report(
                    span,
                    SemanticErrorKind::GenericArgumentCountMismatch {
                        name: p_name,
                        expected: parent_info.generic_params.len(),
                        found: args.len(),
                    },
                );
                return Err(());
            }
            for (i, param_sym) in parent_info.generic_params.iter().enumerate() {
                type_mapping.insert(*param_sym, args[i].clone());
            }
        }

        // 3. 修改子类 (只能修改当前文件的)
        let child_info = self.tables.get_mut(&child_id).unwrap();

        // 3.1 填充字段 (Fields)
        for (f_name, f_info) in &parent_info.fields {
            for (f_name, f_info) in &parent_info.fields {
                // 如果子类没有覆盖该字段，则从父类拷贝
                if !child_info.fields.contains_key(f_name) {
                    let new_type = f_info.ty.substitute(&type_mapping);

                    // [New] 这里的 value 也要处理吗？
                    // 如果 value 表达式里包含泛型（比如 T()），理论上需要 AST 级别的替换。
                    // 但 Loom v0.1 暂时可以只做浅拷贝。如果字段初始值是字面量，这完全没问题。
                    let new_field_info = crate::analyzer::info::FieldInfo {
                        ty: new_type,
                        span: f_info.span,
                        // [Key] 核心修复：Mixin 表达式！
                        value: f_info.value.clone(),
                    };

                    child_info.fields.insert(*f_name, new_field_info);
                }
            }
        }

        // 3.2 填充方法 (Methods)
        for (m_name, m_info) in &parent_info.methods {
            if !child_info.methods.contains_key(m_name) {
                let m_sig = &m_info.signature;
                let new_params = m_sig
                    .params
                    .iter()
                    .map(|(n, t)| (*n, t.substitute(&type_mapping)))
                    .collect();
                let new_ret = m_sig.ret.substitute(&type_mapping);

                let new_sig = FunctionSignature {
                    params: new_params,
                    ret: new_ret,
                    is_abstract: m_sig.is_abstract,
                };

                let new_info = crate::analyzer::info::MethodInfo {
                    generic_params: m_info.generic_params.clone(),
                    signature: new_sig,
                    span: m_info.span,
                };
                child_info.methods.insert(*m_name, new_info);
            }
        }
        Ok(())
    }

    fn get_table_span(&self, id: TableId) -> crate::utils::Span {
        self.tables
            .get(&id)
            .map(|t| t.defined_span)
            .unwrap_or_else(|| crate::utils::Span::default())
    }

    /// [New] 跨文件获取 TableInfo
    /// 无论是本地定义的，还是从其他模块导入的，都能拿到
    fn fetch_table_info(&self, id: TableId) -> Option<crate::analyzer::info::TableInfo> {
        // Case 1: 本地文件
        if id.file_id() == self.current_file_id {
            return self.tables.get(&id).cloned();
        }

        // Case 2: 外部模块
        // 1. 通过 SourceManager 拿到路径
        let path = self.ctx.source_manager.get_file_path(id.file_id())?;

        // 2. 去全局模块缓存里查找
        let module_info = self.ctx.modules.get(path)?;

        // 3. 在那个模块里查找 Table
        // [Fix] 直接用 id 查！不需要 .symbol()
        // 因为 module_info.tables 的 key 现在也是 TableId
        module_info.tables.get(&id).cloned()
    }
}
