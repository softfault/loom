// src/interpreter.rs

pub mod environment;
pub mod errors;
pub mod evaluate;
pub mod native;
pub mod value;

use crate::analyzer::TableId;
use crate::analyzer::resolve_module_path;
use crate::ast::*;
use crate::context::Context;
use crate::source::FileId;
use crate::utils::Symbol;
use environment::Environment;

// [New] 引入具体的错误类型
use errors::RuntimeErrorKind;
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
    // [Refactor] 结构化错误
    Err(RuntimeErrorKind),
    Break,
    Continue,
}

impl EvalResult {
    pub fn into_result(self) -> Result<Value, String> {
        match self {
            EvalResult::Ok(v) => Ok(v),
            EvalResult::Return(v) => Ok(v),
            // 使用 Display trait 格式化错误
            EvalResult::Err(kind) => Err(kind.to_string()),
            // Break/Continue 不能逃逸到函数之外
            EvalResult::Break | EvalResult::Continue => {
                Err("Error: 'break' or 'continue' outside of loop".into())
            }
        }
    }
}

pub struct Interpreter<'a> {
    pub ctx: &'a mut Context,

    // 内置环境 (包含 print, int 等)
    // 这是所有模块的"爷爷"
    pub builtins: Rc<RefCell<Environment>>,

    // 全局环境 -> 指向"当前正在执行的模块的全局环境"
    pub globals: Rc<RefCell<Environment>>,

    // 当前局部环境 (函数调用时会变)
    pub environment: Rc<RefCell<Environment>>,

    // 模块缓存
    // 防止重复加载，解决循环依赖
    // Key: FileId -> Value: Value::Module
    pub module_cache: HashMap<FileId, Value>,

    // 模块完整 AST (有序执行用)
    // Key: FileId -> Value: Rc<Program>
    pub module_programs: HashMap<FileId, Rc<Program>>,

    // AST 注册表
    pub table_definitions: HashMap<TableId, Rc<TableDefinition>>,
    pub function_definitions: HashMap<(FileId, Symbol), Rc<MethodDefinition>>,

    pub current_file_path: PathBuf,
    pub main_file_id: FileId,
    pub current_file_id: FileId, // 当前执行文件 ID
}

impl<'a> Interpreter<'a> {
    pub fn new(ctx: &'a mut Context, main_file_path: PathBuf, main_file_id: FileId) -> Self {
        // 1. 初始化内置环境
        let builtins = Rc::new(RefCell::new(Environment::new()));
        builtins.borrow_mut().define(
            ctx.intern("print"),
            Value::NativeFunction(value::NativeFunc::new("print", native::native_print)),
        );
        // 这里还可以 define("int", Value::Table(primitive_int)) 等

        // 2. 初始化 Main 模块的环境
        // Main 的父环境是 builtins
        let main_env = Rc::new(RefCell::new(Environment::with_enclosing(builtins.clone())));

        let abs_main_path = main_file_path.canonicalize().unwrap_or(main_file_path);

        Self {
            ctx,
            builtins: builtins.clone(),
            globals: main_env.clone(),     // 初始 globals 是 Main
            environment: main_env.clone(), // 初始 env 也是 Main
            module_cache: HashMap::new(),
            table_definitions: HashMap::new(),
            function_definitions: HashMap::new(),
            module_programs: HashMap::new(),
            current_file_path: abs_main_path,
            current_file_id: main_file_id,
            main_file_id,
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<Value, String> {
        // Step 1: 把 Main 放入缓存
        // 这样 Main 自己 import 自己（虽然少见）或者是循环依赖时也能工作
        let main_module_val = Value::Module(self.main_file_id, self.globals.clone());
        self.module_cache.insert(self.main_file_id, main_module_val);

        // Step 2: 执行顶层代码 (定义类、函数、变量、Use)
        // 此时 self.globals 指向 Main 的环境
        self.run_top_level(program)?;

        // Step 3: 执行 Main 入口
        self.run_main_entry()
    }

    // --- Loading / Top Level Phase ---

    fn run_top_level(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.definitions {
            match item {
                TopLevelItem::Table(def) => {
                    let table_id = TableId(self.current_file_id, def.name);
                    let val = Value::Table(table_id);
                    self.globals.borrow_mut().define(def.name, val);
                }

                TopLevelItem::Function(func_def) => {
                    let val = Value::Function(
                        self.current_file_id, // 确保这是当前文件的 ID
                        func_def.name,
                        self.globals.clone(), // <--- 捕获！
                    );
                    self.globals.borrow_mut().define(func_def.name, val);
                }

                TopLevelItem::Field(field_def) => {
                    let val = if let Some(expr) = &field_def.value {
                        match self.evaluate(expr) {
                            EvalResult::Ok(v) => v,
                            // [Fix] 错误转换
                            EvalResult::Err(e) => return Err(e.to_string()),
                            _ => return Err("Control flow error in global init".into()),
                        }
                    } else {
                        Value::Nil
                    };
                    self.globals.borrow_mut().define(field_def.name, val);
                }

                TopLevelItem::Use(stmt) => self.bind_module(stmt)?,
            }
        }
        Ok(())
    }

    // [Refactor] 真正的模块加载逻辑
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
        let target_path = resolve_module_path(self.ctx, &stmt.anchor, &path_segments, current_dir)
            .ok_or_else(|| format!("Module not found: {:?}", path_segments))?;
        let abs_path = target_path.canonicalize().unwrap_or(target_path);

        // 2. 加载文件 (获取 FileId)
        let file_id = self
            .ctx
            .source_manager
            .load_file(&abs_path)
            .map_err(|e| format!("IO Error: {}", e))?;

        // 3. [Check Cache] 检查是否已加载
        let module_val = if let Some(cached) = self.module_cache.get(&file_id) {
            cached.clone()
        } else {
            // 4. [Load & Exec] 如果没加载，执行加载流程
            self.load_and_evaluate_module(file_id, abs_path)?
        };

        // 5. 绑定到当前环境
        self.globals.borrow_mut().define(bind_name, module_val);
        Ok(())
    }

    /// 加载并执行模块
    fn load_and_evaluate_module(
        &mut self,
        file_id: FileId,
        path: PathBuf,
    ) -> Result<Value, String> {
        // A. 创建新环境
        let module_env = Rc::new(RefCell::new(Environment::with_enclosing(
            self.builtins.clone(),
        )));
        let module_val = Value::Module(file_id, module_env.clone());
        self.module_cache.insert(file_id, module_val.clone());

        // B. 保存上下文
        let prev_globals = self.globals.clone();
        let prev_env = self.environment.clone();
        let prev_path = self.current_file_path.clone();
        let prev_file_id = self.current_file_id;

        // C. 切换上下文
        self.globals = module_env.clone();
        self.environment = module_env;
        self.current_file_path = path;
        self.current_file_id = file_id;

        // D. 获取完整 AST
        let program = self.module_programs.get(&file_id).cloned().ok_or_else(|| {
            format!(
                "Runtime: AST for module {:?} not found (Driver issue)",
                file_id
            )
        })?;

        // E. 执行
        let run_result = self.run_top_level(&program);

        // F. 恢复上下文
        self.globals = prev_globals;
        self.environment = prev_env;
        self.current_file_path = prev_path;
        self.current_file_id = prev_file_id;

        run_result?;
        Ok(module_val)
    }

    // --- Execution Phase ---

    fn run_main_entry(&mut self) -> Result<Value, String> {
        let main_sym = self.ctx.intern("main");

        // 1. 在全局环境 (Globals) 中查找 main 函数
        let main_func = match self.globals.borrow().get(main_sym) {
            Some(v) => v,
            None => {
                // 如果没有 main 函数，对于脚本来说也是合法的 (纯副作用脚本)
                return Ok(Value::Unit);
            }
        };

        // 2. 调用 main()
        match self.call_value(main_func, &[], None) {
            EvalResult::Ok(v) => Ok(v),
            EvalResult::Return(v) => Ok(v),
            EvalResult::Err(e) => Err(e.to_string()),
            _ => Err("Runtime Error: Control flow escape".into()),
        }
    }

    /// === 2. 实例化 Table ===
    fn instantiate_table(&mut self, def: &TableDefinition, file_id: FileId) -> EvalResult {
        let mut fields_map = HashMap::new();
        let table_id = TableId(file_id, def.name);

        // --- Step 1: 准备阶段 (只读) ---
        let fields_to_init: Vec<(Symbol, Option<Expression>)> = {
            let file_path = match self.ctx.source_manager.get_file_path(file_id) {
                Some(p) => p,
                None => {
                    return EvalResult::Err(RuntimeErrorKind::Internal("File path missing".into()));
                }
            };
            let mod_info = match self.ctx.modules.get(file_path) {
                Some(m) => m,
                None => {
                    return EvalResult::Err(RuntimeErrorKind::Internal(
                        "ModuleInfo missing".into(),
                    ));
                }
            };
            let table_info = match mod_info.tables.get(&table_id) {
                Some(t) => t,
                None => {
                    return EvalResult::Err(RuntimeErrorKind::Internal("TableInfo missing".into()));
                }
            };
            table_info
                .fields
                .iter()
                .map(|(name, info)| (*name, info.value.clone()))
                .collect()
        };

        // --- Step 2: 执行阶段 (关键修改) ---

        // 1. 保存“案发现场” (Caller's Context)
        // 这是当前正在执行代码的环境 (比如 main.lm)
        let caller_env = self.environment.clone();
        let caller_globals = self.globals.clone();

        // 2. 找到“定义现场” (Definer's Context)
        //我们要找到这个类 (def) 是在哪个文件 (file_id) 定义的，并拿到那个文件的环境
        let definer_env = if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id) {
            env.clone()
        } else {
            // 如果缓存里没有，这通常是不可能的（因为你要用它，肯定已经加载了），
            // 除非是当前文件自己实例化自己，且尚未写入缓存。
            // 兜底策略：如果 file_id 就是当前执行的文件，直接用当前的 globals
            if file_id == self.current_file_id {
                self.globals.clone()
            } else {
                return EvalResult::Err(RuntimeErrorKind::Internal(format!(
                    "Module environment for file {:?} not found in cache",
                    file_id
                )));
            }
        };

        // 3. 切换环境
        // self.environment 控制局部变量查找 (对于字段初始化，通常没有局部变量，但为了一致性设为 def_env)
        // self.globals 控制全局变量查找 (这最重要，决定了 default_hp 找谁)
        self.environment = definer_env.clone();
        self.globals = definer_env;

        // 4. 执行初始化
        // 此时调用 self.evaluate，它眼中的“世界”变成了 lib.lm
        for (name, init_expr_opt) in fields_to_init {
            let value = if let Some(expr) = &init_expr_opt {
                match self.evaluate(expr) {
                    EvalResult::Ok(v) => v,
                    other => {
                        // 出错如果要提前返回，一定要记得恢复环境！
                        self.environment = caller_env;
                        self.globals = caller_globals;
                        return other;
                    }
                }
            } else {
                Value::Nil
            };
            fields_map.insert(name, value);
        }

        // 5. 恢复环境
        self.environment = caller_env;
        self.globals = caller_globals;

        // --- Step 3: 构造实例 ---
        let instance = Rc::new(Instance {
            table_id,
            fields: RefCell::new(fields_map),
        });

        EvalResult::Ok(Value::Instance(instance))
    }

    /// === 通用调用入口 ===
    pub fn call_value(
        &mut self,
        func: Value,
        args: &[Value],
        _receiver: Option<Value>,
    ) -> EvalResult {
        match func {
            // 1. 原生函数调用 (print)
            Value::NativeFunction(f) => match f.call(self.ctx, args) {
                Ok(v) => EvalResult::Ok(v),
                // [Fix] 错误传播
                Err(e) => EvalResult::Err(e),
            },

            // 2. 顶层函数调用 (用户定义)
            Value::Function(file_id, func_name, captured_env) => {
                let func_def = match self.function_definitions.get(&(file_id, func_name)) {
                    Some(def) => def.clone(),
                    None => {
                        return EvalResult::Err(RuntimeErrorKind::Internal(
                            "Function AST not found".to_string(),
                        ));
                    }
                };

                // C. 准备环境 (Context Switch)
                let mut func_env = Environment::with_enclosing(captured_env.clone());

                // 参数检查
                if args.len() != func_def.params.len() {
                    return EvalResult::Err(RuntimeErrorKind::ArgumentCountMismatch {
                        func_name: self.ctx.resolve_symbol(func_name).to_string(),
                        expected: func_def.params.len(),
                        found: args.len(),
                    });
                }

                for (i, param) in func_def.params.iter().enumerate() {
                    func_env.define(param.name, args[i].clone());
                }

                // D. 切换上下文并执行
                let prev_env = self.environment.clone();
                let prev_globals = self.globals.clone(); // 保存当前模块环境

                self.environment = Rc::new(RefCell::new(func_env));
                self.globals = captured_env;

                // E. 执行
                let result = if let Some(body) = &func_def.body {
                    self.execute_block(body)
                } else {
                    EvalResult::Ok(Value::Nil)
                };

                // F. 恢复上下文
                self.environment = prev_env;
                self.globals = prev_globals; // 恢复回调用者的模块环境

                match result {
                    EvalResult::Return(v) => EvalResult::Ok(v),
                    other => other,
                }
            }

            // 3. 错误处理
            _ => EvalResult::Err(RuntimeErrorKind::NotCallable(
                func.to_string(&self.ctx.interner),
            )),
        }
    }

    /// [Core Helper] 获取指定 Table 的父类 TableId
    /// 这一步封装了复杂的跨模块查找逻辑
    fn get_parent_table_id(&self, table_id: TableId) -> Option<TableId> {
        let def = self.table_definitions.get(&table_id)?;

        // 1. 获取父类类型引用
        let parent_ref = def.data.prototype.as_ref()?;

        match &parent_ref.data {
            // Case 1: 本地/已导入的具名引用 (Animal)
            TypeRefData::Named(sym) => {
                // 需要去定义该子类的模块环境中查找父类符号
                // 1. 找到定义该 Table 的模块环境
                let file_id = table_id.file_id();
                if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id) {
                    // 2. 在该模块环境中查找符号
                    if let Some(Value::Table(parent_id)) = env.borrow().get(*sym) {
                        return Some(parent_id);
                    }
                }
                None
            }

            // Case 2: 泛型实例 (List<T>) -> 实际上继承自 Base Table (List)
            TypeRefData::GenericInstance { base, .. } => {
                // 同上，去模块环境找 base
                let file_id = table_id.file_id();
                if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id)
                    && let Some(Value::Table(parent_id)) = env.borrow().get(*base)
                {
                    return Some(parent_id);
                }
                None
            }

            // Case 3: 显式跨模块引用 (lib.Animal)
            TypeRefData::Member { module, member } => {
                let file_id = table_id.file_id();
                if let Some(Value::Module(_, env)) = self.module_cache.get(&file_id) {
                    // 1. 找模块对象 (animal_lib)
                    if let Some(Value::Module(_, mod_env)) = env.borrow().get(*module) {
                        // 2. 找导出类 (Animal)
                        if let Some(Value::Table(parent_id)) = mod_env.borrow().get(*member) {
                            return Some(parent_id);
                        }
                    }
                }
                None
            }

            _ => None,
        }
    }
}
