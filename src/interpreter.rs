// src/interpreter.rs

pub mod environment;
pub mod evaluate;
pub mod native;
pub mod value;

use crate::analyzer::TableId; // [New]
use crate::analyzer::resolve_module_path;
use crate::ast::*;
use crate::context::Context;
use crate::source::FileId; // [New]
use crate::utils::Symbol;
use environment::Environment;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use value::{Instance, Value};

/// 解释器的求值结果
#[derive(Debug, Clone)]
pub enum EvalResult {
    Ok(Value),
    Return(Value),
    Err(String),
    Break,
    Continue,
}

impl EvalResult {
    pub fn into_result(self) -> Result<Value, String> {
        match self {
            EvalResult::Ok(v) => Ok(v),
            EvalResult::Return(v) => Ok(v),
            EvalResult::Err(e) => Err(e),
            // Break/Continue 不能逃逸到函数之外
            EvalResult::Break | EvalResult::Continue => {
                Err("Error: 'break' or 'continue' outside of loop".into())
            }
        }
    }
}

pub struct Interpreter<'a> {
    pub ctx: &'a mut Context,

    // 全局环境
    pub globals: Rc<RefCell<Environment>>,

    // 当前环境
    pub environment: Rc<RefCell<Environment>>,

    // [修改] Table 定义注册表
    // Key: TableId (FileId, Symbol) -> Value: Rc<AST>
    // 这些定义由 Driver 在启动前注入 (包括 Main 和所有 Module)
    pub table_definitions: HashMap<TableId, Rc<TableDefinition>>,

    // 当前正在执行的文件的路径 (用于解析相对路径 import)
    pub current_file_path: PathBuf,

    // [New] 主文件的 ID，用于寻找入口 Main Table
    pub main_file_id: FileId,
}

impl<'a> Interpreter<'a> {
    // [修改] 增加 main_file_id 参数
    pub fn new(ctx: &'a mut Context, main_file_path: PathBuf, main_file_id: FileId) -> Self {
        let globals = Rc::new(RefCell::new(Environment::new()));

        globals.borrow_mut().define(
            ctx.intern("print"),
            Value::NativeFunction(crate::interpreter::native::native_print),
        );

        // 依然保留 path 处理，因为 resolve_module_path 需要用到 current_file_path
        // 但不再调用 source_manager.load_file 了
        let abs_main_path = main_file_path.canonicalize().unwrap_or(main_file_path);

        Self {
            ctx,
            globals: globals.clone(),
            environment: globals,
            table_definitions: HashMap::new(),
            current_file_path: abs_main_path,
            main_file_id, // 直接使用传入的 ID
        }
    }

    /// === 1. 程序入口 ===
    pub fn eval_program(&mut self, program: &Program) -> Result<Value, String> {
        // Step 1: 执行顶层语句 (主要处理 use)
        // 注意：Table 定义已经由 Driver 注入到了 self.table_definitions，
        // 所以这里只需要处理 Use 语句来绑定变量。
        self.run_top_level(program)?;

        // Step 2: 执行入口 [Main]
        self.run_main_entry()
    }

    // --- Loading / Top Level Phase ---

    fn run_top_level(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.definitions {
            match item {
                TopLevelItem::Table(def) => {
                    // [Fix] 将当前文件定义的 Table 注册到全局变量中
                    // 这样代码里才能直接使用 Dog()
                    // 使用 self.main_file_id，因为 run_top_level 目前只运行主程序
                    let table_id = TableId(self.main_file_id, def.name);
                    let val = Value::Table(table_id);

                    // 注册到全局环境
                    self.globals.borrow_mut().define(def.name, val);
                }
                TopLevelItem::Use(stmt) => self.bind_module(stmt)?,
            }
        }
        Ok(())
    }

    fn bind_module(&mut self, stmt: &UseStatement) -> Result<(), String> {
        let module_name_sym = stmt.path.last().unwrap();
        let bind_name = stmt.alias.unwrap_or(*module_name_sym);

        let path_segments: Vec<String> = stmt
            .path
            .iter()
            .map(|s| self.ctx.resolve_symbol(*s).to_string())
            .collect();

        let current_dir = self
            .current_file_path
            .parent()
            .unwrap_or(&self.ctx.root_dir);

        // 1. 解析路径
        if let Some(target_path) =
            resolve_module_path(self.ctx, &stmt.anchor, &path_segments, current_dir)
        {
            let abs_path = target_path.canonicalize().unwrap_or(target_path);

            // 2. [关键] 获取 FileId
            // Driver 已经加载过这个文件了，load_file 会直接返回已有的 ID
            let file_id = self
                .ctx
                .source_manager
                .load_file(&abs_path)
                .map_err(|e| format!("Failed to load module file: {}", e))?;

            // 3. 创建 Value::Module(FileId)
            let module_val = Value::Module(file_id);

            // 4. 绑定到全局变量
            self.globals.borrow_mut().define(bind_name, module_val);
            Ok(())
        } else {
            Err(format!(
                "Runtime Error: Module not found {:?} (searched from {:?})",
                path_segments, current_dir
            ))
        }
    }

    // --- Execution Phase ---

    fn run_main_entry(&mut self) -> Result<Value, String> {
        let main_sym = self.ctx.intern("Main");
        let main_table_id = TableId(self.main_file_id, main_sym);

        // 查找定义
        let main_def = self
            .table_definitions
            .get(&main_table_id)
            .cloned()
            .ok_or_else(|| "Runtime Error: No [Main] table found in entry file.".to_string())?;

        // 实例化 Main
        let main_instance = match self.instantiate_table(&main_def, self.main_file_id) {
            EvalResult::Ok(v) => v,
            // 实例化过程中不能 return, break, continue
            EvalResult::Return(_) | EvalResult::Break | EvalResult::Continue => {
                return Err(
                    "Runtime Error: Unexpected control flow during Main instantiation".into(),
                );
            }
            EvalResult::Err(e) => return Err(e),
        };

        let main_method_name = self.ctx.intern("main");

        // 调用 main()
        match self.call_method(main_instance, main_method_name, &[]) {
            // main 函数正常执行完毕
            EvalResult::Ok(v) => Ok(v),
            // main 函数显式 return
            EvalResult::Return(v) => Ok(v),

            // 错误：break/continue 逃逸到了 main 之外
            EvalResult::Break | EvalResult::Continue => {
                Err("Runtime Error: 'break' or 'continue' found outside of loop".into())
            }
            EvalResult::Err(e) => Err(e),
        }
    }

    /// === 2. 实例化 Table ===
    /// file_id: 该 Table 定义所在的文件 ID (用于构造 Instance 的 TableId)
    fn instantiate_table(&mut self, def: &TableDefinition, file_id: FileId) -> EvalResult {
        let mut fields = HashMap::new();

        let caller_env = self.environment.clone();

        // 切换到全局环境执行字段初始化
        self.environment = self.globals.clone();

        for item in &def.data.items {
            if let TableItem::Field(field_def) = item {
                let value = if let Some(expr) = &field_def.value {
                    match self.evaluate(expr) {
                        EvalResult::Ok(v) => v,

                        // [Fix] 处理所有控制流逃逸
                        // 字段初始化不能 Return，也不能 Break/Continue
                        EvalResult::Return(_) | EvalResult::Break | EvalResult::Continue => {
                            self.environment = caller_env;
                            return EvalResult::Err(
                                "Cannot use 'return', 'break', or 'continue' in field initialization".into(),
                            );
                        }

                        EvalResult::Err(e) => {
                            // !!! 必须恢复环境 !!!
                            self.environment = caller_env;
                            return EvalResult::Err(e);
                        }
                    }
                } else {
                    Value::Nil
                };
                fields.insert(field_def.name, value);
            }
        }

        self.environment = caller_env;

        // [修改] Instance 现在存储 TableId
        let instance = Rc::new(Instance {
            table_id: TableId(file_id, def.name), // 唯一 ID
            fields: RefCell::new(fields),
        });

        EvalResult::Ok(Value::Instance(instance))
    }

    /// === 3. 调用方法 ===
    fn call_method(&mut self, receiver: Value, method_name: Symbol, args: &[Value]) -> EvalResult {
        let instance = match &receiver {
            Value::Instance(i) => i.clone(),
            _ => return EvalResult::Err("Cannot call method on non-instance".into()),
        };

        // [修改] 使用 instance.table_id 去查表
        let method_def = {
            // 通过 TableId 查找 TableAST
            let table_def = match self.table_definitions.get(&instance.table_id) {
                Some(d) => d,
                None => {
                    let t_name = self.ctx.resolve_symbol(instance.table_id.symbol());
                    return EvalResult::Err(format!(
                        "Runtime Def Missing: Table '{}' definition not found",
                        t_name
                    ));
                }
            };

            let found = table_def.items.iter().find_map(|item| {
                if let TableItem::Method(m) = item {
                    if m.name == method_name {
                        return Some(m);
                    }
                }
                None
            });

            match found {
                Some(m) => m.clone(),
                None => {
                    let m_name = self.ctx.resolve_symbol(method_name);
                    let t_name = self.ctx.resolve_symbol(instance.table_id.symbol());
                    return EvalResult::Err(format!(
                        "Method '{}' not found on class '{}'",
                        m_name, t_name
                    ));
                }
            }
        };

        // 准备环境
        let mut method_env = Environment::with_enclosing(self.globals.clone());
        // [Key] self 绑定为 Instance
        method_env.define(self.ctx.intern("self"), receiver.clone());

        if args.len() != method_def.params.len() {
            return EvalResult::Err(format!(
                "Arg count mismatch: expected {}, got {}",
                method_def.params.len(),
                args.len()
            ));
        }
        for (i, param) in method_def.params.iter().enumerate() {
            method_env.define(param.name, args[i].clone());
        }

        let prev_env = self.environment.clone();
        self.environment = Rc::new(RefCell::new(method_env));

        let result = if let Some(body) = &method_def.body {
            self.execute_block(body)
        } else {
            EvalResult::Err("Cannot call abstract method".into())
        };

        self.environment = prev_env;

        match result {
            EvalResult::Return(v) => EvalResult::Ok(v),
            other => other,
        }
    }
}
