use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::{Analyzer, FunctionSignature, Type};
use crate::utils::Symbol;
use std::collections::{HashMap, HashSet};

impl<'a> Analyzer<'a> {
    /// Pass 2 入口：解析继承层次与填充
    pub fn resolve_hierarchy(&mut self) {
        // 记录已完成填充的 Table
        let mut resolved = HashSet::new();
        // 记录正在递归路径上的 Table (用于检测 A -> B -> A)
        let mut visiting = HashSet::new();

        // 获取所有 Table 的名字 (先收集 keys 以避免对 self.tables 的借用冲突)
        let table_names: Vec<Symbol> = self.tables.keys().cloned().collect();

        for name in table_names {
            // 我们忽略 Result 的返回值，因为错误已经在 resolve_table 内部 report 了
            let _ = self.resolve_table(name, &mut resolved, &mut visiting);
        }
    }

    /// 递归解析单个 Table
    /// 返回 Result<(), ()> 仅仅用于控制递归流程（出错时提前终止），实际错误信息已上报
    fn resolve_table(
        &mut self,
        name: Symbol,
        resolved: &mut HashSet<Symbol>,
        visiting: &mut HashSet<Symbol>,
    ) -> Result<(), ()> {
        // 1. 如果已经处理过，直接返回成功
        if resolved.contains(&name) {
            return Ok(());
        }

        // 2. 循环检测
        if visiting.contains(&name) {
            // 获取 Span 用于报错
            let span = self.get_table_span(name);
            let name_str = self.ctx.resolve_symbol(name).to_string();

            self.report(span, SemanticErrorKind::CyclicInheritance(name_str));
            return Err(());
        }

        visiting.insert(name);

        // 3. 获取父类信息
        // [Key] Clone 出 Type，彻底断开对 self.tables 的借用
        // 这样后续递归调用 resolve_table (需要 &mut self) 就不会冲突
        let parent_type_opt = self.tables.get(&name).and_then(|t| t.parent.clone());

        if let Some(parent_type) = parent_type_opt {
            // 3.1 提取父类 Symbol
            let parent_sym = match parent_type.get_base_symbol() {
                Some(s) => s,
                None => {
                    let span = self.get_table_span(name);
                    let type_str = parent_type.display(&self.ctx.interner).to_string();

                    self.report(span, SemanticErrorKind::InvalidParentType(type_str));

                    visiting.remove(&name);
                    return Err(());
                }
            };

            // 3.2 检查父类是否存在
            if !self.tables.contains_key(&parent_sym) {
                let span = self.get_table_span(name);
                let p_name = self.ctx.resolve_symbol(parent_sym).to_string();

                self.report(span, SemanticErrorKind::UndefinedSymbol(p_name));

                visiting.remove(&name);
                return Err(());
            }

            // 3.3 递归：先确保父类已经 Resolved
            // 如果父类解析失败，我们也失败
            if self.resolve_table(parent_sym, resolved, visiting).is_err() {
                visiting.remove(&name);
                return Err(());
            }

            // 3.4 执行填充 (The Filling Magic)
            if self.fill_from_parent(name, &parent_type).is_err() {
                visiting.remove(&name);
                return Err(());
            }
        }

        // 成功完成
        visiting.remove(&name);
        resolved.insert(name);
        Ok(())
    }

    /// 从父类拷贝字段/方法到子类，并应用泛型替换
    fn fill_from_parent(&mut self, child_name: Symbol, parent_type: &Type) -> Result<(), ()> {
        // 1. 获取父类数据 (需要 Clone，因为我们要修改 Child，同时需要读取 Parent)
        let parent_sym = parent_type.get_base_symbol().unwrap(); // 之前 check 过了，unwrap 安全
        let parent_info = self.tables.get(&parent_sym).unwrap().clone();

        // 2. 计算泛型替换映射 (Substitution Map)
        // 例如 [Child: Base<int>] (parent_type) vs [Base<T>] (parent_info)
        // Mapping: T -> int
        let mut type_mapping = HashMap::new();

        if let Type::GenericInstance { args, .. } = parent_type {
            // 2.1 检查泛型参数数量
            if args.len() != parent_info.generic_params.len() {
                let span = self.get_table_span(child_name);
                let p_name = self.ctx.resolve_symbol(parent_sym).to_string();

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

            // 2.2 建立映射
            for (i, param_sym) in parent_info.generic_params.iter().enumerate() {
                type_mapping.insert(*param_sym, args[i].clone());
            }
        }

        // 3. 开始填充 (Modify Child)
        // 获取 Child 的可变借用
        let child_info = self.tables.get_mut(&child_name).unwrap();

        // 3.1 填充字段 (Fields)
        for (f_name, f_type) in &parent_info.fields {
            if !child_info.fields.contains_key(f_name) {
                // Child 没定义 -> 继承 (拷贝并替换泛型)
                let new_type = f_type.substitute(&type_mapping);
                child_info.fields.insert(*f_name, new_type);
            }
            // 如果 Child 已经定义了，则是 Override/Shadowing，这里暂不检查类型兼容性
            // 类型兼容性检查放在 Pass 3 (Check) 中进行
        }

        // 3.2 填充方法 (Methods)
        for (m_name, m_sig) in &parent_info.methods {
            if !child_info.methods.contains_key(m_name) {
                // Child 没定义 -> 继承
                let new_params = m_sig
                    .params
                    .iter()
                    .map(|(n, t)| (*n, t.substitute(&type_mapping)))
                    .collect();

                let new_ret = m_sig.ret.substitute(&type_mapping);

                let new_sig = FunctionSignature {
                    params: new_params,
                    ret: new_ret,
                    is_abstract: false, // 继承下来的默认非抽象，除非父类本身就是抽象且没实现？
                                        // 实际上这里我们只拷贝签名，实现(Body)是在 Interpreter 查找时去父类找
                };
                child_info.methods.insert(*m_name, new_sig);
            }
        }

        Ok(())
    }

    fn get_table_span(&self, table: Symbol) -> crate::utils::Span {
        self.tables
            .get(&table)
            .map(|t| t.defined_span)
            .unwrap_or_else(|| crate::utils::Span::default()) // Fallback，理论上不应发生
    }
}
