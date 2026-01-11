use super::*;
use crate::analyzer::TableId;

impl<'a> Analyzer<'a> {
    pub fn check_type_compatibility(&self, target: &Type, source: &Type) -> bool {
        // 1. 基础检查 (Int, Str, Bool, Any 等)
        if target.is_assignable_from(source) {
            return true;
        }

        match (target, source) {
            // Case A: 普通类的继承 (Animal vs Dog)
            // [Fix] 使用 TableId
            (Type::Table(target_id), Type::Table(source_id)) => {
                self.is_subtype(*source_id, *target_id)
            }

            // Case B: 泛型实例的继承 (List<Animal> vs List<Dog>)
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
                // 2.1 基础类必须相同 (List vs List)
                // [Fix] 比较 TableId
                if target_base != source_base {
                    // 未来可以扩展：允许 ArrayList<T> 赋值给 List<T>
                    // return self.is_subtype(*source_base, *target_base);
                    return false;
                }

                // 2.2 参数数量必须一致
                if target_args.len() != source_args.len() {
                    return false;
                }

                // 2.3 泛型参数协变检查
                for (t_arg, s_arg) in target_args.iter().zip(source_args.iter()) {
                    if !self.check_type_compatibility(t_arg, s_arg) {
                        return false;
                    }
                }
                true
            }

            // Case C: 模块类型兼容性 (通常模块不能赋值，或者是单例)
            // (Type::Module(id1), Type::Module(id2)) => id1 == id2,
            // Case D: 数组协变检查
            // 允许 [Dog] 赋值给 [Animal]
            (Type::Array(target_inner), Type::Array(source_inner)) => {
                // 递归调用 check_type_compatibility
                // 这样能利用已有的 is_subtype 逻辑处理内部元素
                self.check_type_compatibility(target_inner, source_inner)
            }
            _ => false,
        }
    }

    /// [Refactor] 递归检查 source 是否继承自 target
    /// 使用 TableId 进行精确匹配 (解决同名不同文件的问题)
    pub fn is_subtype(&self, child_id: TableId, target_id: TableId) -> bool {
        // 递归基：ID 完全相等 (同一个文件的同一个类)
        if child_id == target_id {
            return true;
        }

        // 1. 查找 Child 的定义 (支持跨文件查找)
        if let Some(info) = self.find_table_info(child_id) {
            // 2. 看看它有没有父类
            if let Some(parent_type) = &info.parent {
                // 3. 提取父类 ID
                // 注意：resolve 阶段已经保证 parent 是 Table 或 GenericInstance
                let parent_id = match parent_type {
                    Type::Table(id) => *id,
                    Type::GenericInstance { base, .. } => *base,
                    _ => return false,
                };

                // 4. 递归检查
                return self.is_subtype(parent_id, target_id);
            }
        }

        false
    }

    /// 通用辅助函数：报告类型不匹配错误
    pub fn error_type_mismatch(&mut self, span: crate::utils::Span, expected: &Type, found: &Type) {
        // 如果其中一个是 Error 类型，通常意味着之前已经报过错了，为了防止报错刷屏，这里选择静默
        if *expected == Type::Error || *found == Type::Error {
            return;
        }

        let expected_str = expected.display(self.ctx).to_string();
        let found_str = found.display(self.ctx).to_string();

        self.report(
            span,
            SemanticErrorKind::TypeMismatch {
                expected: expected_str,
                found: found_str,
            },
        );
    }
}
