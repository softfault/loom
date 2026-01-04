use super::*;
use crate::source::FileId;

impl<'a> Interpreter<'a> {
    /// === 核心入口：字段/方法访问 ===
    pub fn eval_field_access(&mut self, target: &Expression, field: Symbol) -> EvalResult {
        // 1. 先计算 target 的值
        let target_val = require_ok!(self.evaluate(target));

        // 2. 根据值的类型分发处理逻辑
        match target_val {
            Value::Instance(instance) => self.access_instance_member(instance, field),
            Value::Module(file_id) => self.access_module_member(file_id, field),
            // 将 String 和 Array 统一归类为原生类型处理
            val @ Value::Str(_) | val @ Value::Array(_) => self.access_native_member(val, field),
            _ => {
                let field_name = self.ctx.resolve_symbol(field);
                EvalResult::Err(format!(
                    "Cannot access property '{}' on {:?}",
                    field_name, target_val
                ))
            }
        }
    }

    /// === 辅助函数 1：处理实例成员 (Instance) ===
    fn access_instance_member(&mut self, instance: Rc<Instance>, field: Symbol) -> EvalResult {
        // 1. 优先查找实例自身的字段 (Fields)
        // 字段存在于实例对象中，不需要查表
        if let Some(val) = instance.fields.borrow().get(&field) {
            return EvalResult::Ok(val.clone());
        }

        // 2. 查找方法 (Methods) - 支持继承链
        // 委托给专门的查找函数
        if let Some(method_def) = self.find_method_in_chain(instance.table_id, field) {
            return EvalResult::Ok(Value::BoundMethod(instance.clone(), method_def));
        }

        let field_name = self.ctx.resolve_symbol(field);
        EvalResult::Err(format!(
            "Property or method '{}' not found on instance",
            field_name
        ))
    }

    /// === 辅助函数 2：继承链查找核心逻辑 ===
    /// 从 start_id 开始，沿着原型链向上查找方法定义
    fn find_method_in_chain(
        &self,
        start_id: TableId,
        method_name: Symbol,
    ) -> Option<MethodDefinition> {
        let mut current_table_id = start_id;

        // 循环向上查找
        loop {
            // A. 获取当前类的 AST 定义
            // 如果连定义都找不到(比如 file_id 错误)，直接中断
            let table_def = self.table_definitions.get(&current_table_id)?;

            // B. 在当前类中查找方法
            // 注意：使用 .data.items 访问 AST 数据
            for item in &table_def.data.items {
                if let TableItem::Method(method_def) = item {
                    if method_def.name == method_name {
                        // 找到了！直接返回克隆的定义
                        return Some(method_def.clone());
                    }
                }
            }

            // C. 没找到，尝试解析父类 (Prototype)
            // 注意：使用 .data.prototype 访问父类引用
            if let Some(parent_type_ref) = &table_def.data.prototype {
                match &parent_type_ref.data {
                    // Case 1: 简单的命名引用 [Dog : Animal]
                    TypeRefData::Named(parent_sym) => {
                        // 去全局变量环境 (Globals) 里查找这个符号
                        // 因为 Driver 已经在 run_top_level 把所有 Table 注册为全局变量了
                        if let Some(Value::Table(parent_id)) =
                            self.globals.borrow().get(*parent_sym)
                        {
                            current_table_id = parent_id; // 切换到父类 ID，进入下一次循环
                            continue;
                        }
                        // 如果全局变量里没找到名为 parent_sym 的 Table，说明继承链断裂
                        break;
                    }

                    // Case 2: 泛型实例 [IntList : List<int>]
                    // 需要从 GenericInstance 中提取 base (List)
                    TypeRefData::GenericInstance { base, .. } => {
                        // base 是 Symbol，同理去 Globals 找
                        if let Some(Value::Table(parent_id)) = self.globals.borrow().get(*base) {
                            current_table_id = parent_id;
                            continue;
                        }
                        break;
                    }

                    // Case 3: 结构化类型或其他 -> 不支持作为父类
                    _ => break,
                }
            } else {
                // 没有父类，查找结束
                break;
            }
        }

        None
    }

    /// === 辅助函数 3：处理模块导出 ===
    fn access_module_member(&self, file_id: FileId, field: Symbol) -> EvalResult {
        // 构造目标 Table 的唯一 ID
        let target_table_id = TableId(file_id, field);

        // 检查 Interpreter 是否加载了该定义
        if self.table_definitions.contains_key(&target_table_id) {
            return EvalResult::Ok(Value::Table(target_table_id));
        }

        let field_name = self.ctx.resolve_symbol(field);
        EvalResult::Err(format!("Module does not export '{}'", field_name))
    }

    /// === 辅助函数 4：处理原生类型方法 ===
    fn access_native_member(&mut self, target_val: Value, field: Symbol) -> EvalResult {
        let field_name = self.ctx.resolve_symbol(field);

        match target_val {
            Value::Str(_) => match field_name {
                "len" => EvalResult::Ok(Value::BoundNativeMethod(
                    Box::new(target_val),
                    crate::interpreter::native::native_str_len,
                )),
                _ => EvalResult::Err(format!("String has no property '{}'", field_name)),
            },

            Value::Array(_) => match field_name {
                "len" => EvalResult::Ok(Value::BoundNativeMethod(
                    Box::new(target_val),
                    crate::interpreter::native::native_array_len,
                )),
                "push" => EvalResult::Ok(Value::BoundNativeMethod(
                    Box::new(target_val),
                    crate::interpreter::native::native_array_push,
                )),
                _ => EvalResult::Err(format!("Array has no property '{}'", field_name)),
            },

            _ => unreachable!("Should only be called for native types"),
        }
    }
}
