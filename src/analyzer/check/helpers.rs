use super::*;

impl<'a> Analyzer<'a> {
    pub fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        // 1. 基础检查
        if target.is_assignable_from(source) {
            return true;
        }

        // 2. 复杂类型检查
        match (target, source) {
            // Case A: 普通类的继承 (Animal vs Dog)
            (Type::Table(target_id), Type::Table(source_id)) => {
                self.is_subtype(source_id.symbol(), target_id.symbol())
            }

            // Case B: 泛型实例的继承 (List<Animal> vs List<Dog>)
            // [New] 泛型协变检查
            (
                Type::GenericInstance {
                    base: target_base,
                    args: target_args,
                },
                Type::GenericInstance {
                    base: source_base,
                    args: source_args,
                },
            ) => {
                // 2.1 基础类必须相同 (或者存在继承关系，暂时只处理相同的情况)
                // 例如: 不能把 Map<String> 赋值给 List<String>
                if target_base.symbol() != source_base.symbol() {
                    return false;
                }

                // 2.2 参数数量必须一致
                if target_args.len() != source_args.len() {
                    return false;
                }

                // 2.3 [核心] 逐个检查泛型参数的兼容性 (协变)
                // 规则：Source 的参数必须是 Target 参数的子类
                // 即: List<Dog> 可以赋值给 List<Animal>，因为 Dog 是 Animal
                for (t_arg, s_arg) in target_args.iter().zip(source_args.iter()) {
                    // 递归调用 check_type_compatibility
                    if !self.check_type_compatibility(t_arg, s_arg) {
                        return false;
                    }
                }

                true
            }

            // Case C: 数组的协变 (Array<Dog> -> Array<Animal>)
            // 如果你的 Type::Array 是独立的枚举，也要加类似逻辑
            (Type::Array(t_inner), Type::Array(s_inner)) => {
                self.check_type_compatibility(t_inner, s_inner)
            }

            _ => false,
        }
    }

    /// [New] 递归检查 source 是否继承自 target
    pub fn is_subtype(&self, child_sym: Symbol, target_sym: Symbol) -> bool {
        // 递归基：如果 ID 相等，就是子类型 (Self is subtype of Self)
        if child_sym == target_sym {
            return true;
        }

        // 查找 Child 的定义
        if let Some(info) = self.tables.get(&child_sym) {
            // 看看它有没有父类
            if let Some(parent_type) = &info.parent {
                // 获取父类的 Symbol
                if let Some(parent_sym) = parent_type.get_base_symbol() {
                    // 递归检查：父类是不是目标类型的子类？
                    return self.is_subtype(parent_sym, target_sym);
                }
            }
        }

        // 查不到定义或没有父类，说明继承链断了，匹配失败
        false
    }

    /// 通用辅助函数：报告类型不匹配错误
    pub fn error_type_mismatch(&mut self, span: crate::utils::Span, expected: &Type, found: &Type) {
        // 如果其中一个是 Error 类型，通常意味着之前已经报过错了，为了防止报错刷屏，这里选择静默
        if *expected == Type::Error || *found == Type::Error {
            return;
        }

        let expected_str = expected.display(&self.ctx.interner).to_string();
        let found_str = found.display(&self.ctx.interner).to_string();

        self.report(
            span,
            SemanticErrorKind::TypeMismatch {
                expected: expected_str,
                found: found_str,
            },
        );
    }
}
