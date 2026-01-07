use super::*;
use crate::interpreter::errors::RuntimeErrorKind;
use crate::source::FileId;

impl<'a> Interpreter<'a> {
    /// === 核心入口：字段/方法访问 ===
    pub fn eval_field_access(&mut self, target: &Expression, field: Symbol) -> EvalResult {
        // 1. 先计算 target 的值
        let target_val = require_ok!(self.evaluate(target));

        // 2. 根据值的类型分发处理逻辑
        match target_val {
            Value::Instance(instance) => self.access_instance_member(instance, field),

            // [Modified] 匹配新的 Module 结构 (FileId, Env)
            Value::Module(file_id, _) => self.access_module_member(file_id, field),

            // 将 String 和 Array 统一归类为原生类型处理
            val @ Value::Str(_) | val @ Value::Array(_) => self.access_native_member(val, field),

            _ => {
                let field_name = self.ctx.resolve_symbol(field);
                // 试图访问比如 1.length，这是类型错误
                EvalResult::Err(RuntimeErrorKind::TypeError {
                    expected: "Instance, Module, String or Array".into(),
                    found: format!("{:?}", target_val), // 简单描述实际类型
                })
            }
        }
    }

    /// === 辅助函数 1：处理实例成员 (Instance) ===
    fn access_instance_member(&mut self, instance: Rc<Instance>, field: Symbol) -> EvalResult {
        // 1. 优先查找实例自身的字段 (Fields)
        if let Some(val) = instance.fields.borrow().get(&field) {
            return EvalResult::Ok(val.clone());
        }

        // 2. 查找方法 (Methods) - 支持继承链
        // [Modified] find_method_in_chain 现在返回 (MethodDefinition, Env)
        if let Some((method_def, def_env)) = self.find_method_in_chain(instance.table_id, field) {
            // [Fix] 将环境打包进 BoundMethod
            return EvalResult::Ok(Value::BoundMethod(instance.clone(), method_def, def_env));
        }

        let field_name = self.ctx.resolve_symbol(field);

        // [New] 结构化错误
        EvalResult::Err(RuntimeErrorKind::PropertyNotFound {
            target_type: "Instance".into(),
            property: field_name.to_string(),
        })
    }

    /// === 辅助函数 2：继承链查找核心逻辑 ===
    fn find_method_in_chain(
        &self,
        start_id: TableId,
        method_name: Symbol,
    ) -> Option<(MethodDefinition, Rc<RefCell<Environment>>)> {
        let mut current_table_id = start_id;

        loop {
            // A. 获取当前类的 AST 定义
            let table_def = self.table_definitions.get(&current_table_id)?;

            // B. 在当前类中查找方法
            for item in &table_def.data.items {
                if let TableItem::Method(method_def) = item {
                    if method_def.name == method_name {
                        // [Key Step] 找到了方法！现在查找它所属的模块环境
                        let file_id = current_table_id.file_id();

                        // 从缓存中获取环境
                        // 注意：这里必须能找到，因为代码能在运行说明模块肯定加载了
                        if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id) {
                            return Some((method_def.clone(), env.clone()));
                        } else {
                            // 理论上不可能发生，除非 module_cache 被破坏
                            return None;
                        }
                    }
                }
            }

            // C. 没找到，尝试解析父类 (Prototype)
            if let Some(parent_type_ref) = &table_def.data.prototype {
                match &parent_type_ref.data {
                    TypeRefData::Named(parent_sym) => {
                        // 去全局变量环境查找父类
                        if let Some(Value::Table(parent_id)) =
                            self.globals.borrow().get(*parent_sym)
                        {
                            current_table_id = parent_id;
                            continue;
                        }
                        break;
                    }

                    TypeRefData::GenericInstance { base, .. } => {
                        if let Some(Value::Table(parent_id)) = self.globals.borrow().get(*base) {
                            current_table_id = parent_id;
                            continue;
                        }
                        break;
                    }

                    // Case 2: [New] 跨模块引用 [Dog : lib.Animal]
                    TypeRefData::Member { module, member } => {
                        // 1. 先在当前全局变量里找到模块对象 (比如 "animal_lib")
                        // 这一步能成功是因为 'use animal_lib' 会在 globals 里注册一个 Value::Module
                        if let Some(Value::Module(_, mod_env)) = self.globals.borrow().get(*module)
                        {
                            // 2. 去那个模块的环境里找到类 (比如 "Animal")
                            if let Some(Value::Table(parent_id)) = mod_env.borrow().get(*member) {
                                // 3. 切换到父类的 ID，进入下一次循环
                                current_table_id = parent_id;
                                continue;
                            }
                        }
                        // 找不到模块或找不到类，断裂
                        break;
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        None
    }

    /// === 辅助函数 3：处理模块导出 ===
    fn access_module_member(&self, file_id: FileId, field: Symbol) -> EvalResult {
        // 查 module_cache 里的 Environment
        if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id) {
            if let Some(val) = env.borrow().get(field) {
                return EvalResult::Ok(val);
            }

            let field_name = self.ctx.resolve_symbol(field);

            // [New] 结构化错误
            return EvalResult::Err(RuntimeErrorKind::PropertyNotFound {
                target_type: "Module".into(),
                property: field_name.to_string(),
            });
        }

        // [New] 结构化错误
        EvalResult::Err(RuntimeErrorKind::Internal(
            "Module not loaded correctly in cache".into(),
        ))
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
                _ => EvalResult::Err(RuntimeErrorKind::PropertyNotFound {
                    target_type: "String".into(),
                    property: field_name.to_string(),
                }),
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
                _ => EvalResult::Err(RuntimeErrorKind::PropertyNotFound {
                    target_type: "Array".into(),
                    property: field_name.to_string(),
                }),
            },

            _ => unreachable!("Should only be called for native types"),
        }
    }
}
