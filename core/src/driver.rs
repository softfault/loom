use crate::analyzer::{Analyzer, SemanticError, TableId};
use crate::ast::TopLevelItem;
use crate::context::Context;
use crate::interpreter::Interpreter;
use crate::interpreter::value::Value;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::source::FileId;
use crate::utils::Span;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Driver {
    pub ctx: Context,
}

impl Driver {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            ctx: Context::new(root_dir),
        }
    }

    /// 入口：运行一个文件
    pub fn run_file(&mut self, path: &Path) -> Result<Value, String> {
        // 1. 转为绝对路径 (Canonicalize)
        // 这一步必须做，确保 FileId 的唯一性基于绝对路径
        let abs_path = path
            .canonicalize()
            .map_err(|e| format!("Invalid path {:?}: {}", path, e))?;

        // 2. 读取源码
        // SourceManager::load_file 会处理读取和去重逻辑
        // 我们直接复用它，而不是手动 fs::read
        let file_id = self
            .ctx
            .source_manager
            .load_file(&abs_path)
            .map_err(|e| format!("Could not load file {:?}: {}", abs_path, e))?;

        // 3. 执行管线
        self.run_pipeline(file_id, abs_path)
    }

    // core/src/driver.rs

    /// 核心编译管线
    fn run_pipeline(&mut self, file_id: FileId, path: PathBuf) -> Result<Value, String> {
        let source = self.ctx.source_manager.get_file(file_id).src.as_str();

        // ==========================================
        // Step 1: Parsing (语法解析)
        // ==========================================
        let lexer = Lexer::new(source);
        let mut parser = Parser::new(source, lexer, file_id, &mut self.ctx.interner);

        // 解析出主程序的 AST (Owned)
        let program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("Parse Error: {}", e.message);
                return Err(self.format_diagnostic(file_id, e.span, &msg));
            }
        };

        if !parser.errors.is_empty() {
            let mut error_msgs = Vec::new();
            for err in &parser.errors {
                error_msgs.push(self.format_diagnostic(
                    file_id,
                    err.span,
                    &format!("Syntax Error: {}", err.message),
                ));
            }
            return Err(error_msgs.join("\n\n"));
        }

        // ==========================================
        // Step 2: Analysis (语义分析)
        // ==========================================
        let mut analyzer = Analyzer::new(&mut self.ctx, file_id);

        // 注意：analyzer 会递归加载 import 的模块，并把它们的 AST 缓存在 ctx.modules 里
        analyzer.collect_program(&program);
        analyzer.resolve_hierarchy();
        analyzer.check_program(&program);

        if !analyzer.errors.is_empty() {
            let error_msgs: Vec<String> = analyzer
                .errors
                .iter()
                .map(|e| self.format_semantic_error(e))
                .collect();
            return Err(error_msgs.join("\n\n"));
        }

        // ==========================================
        // Step 3: Interpretation (解释执行)
        // ==========================================

        // 准备 Interpreter 所需的数据结构
        let mut table_defs = HashMap::new();
        let mut func_defs = HashMap::new();
        let mut module_programs = HashMap::new();

        // --- 3.1 注入已加载模块 (Modules) 的定义 ---
        // 这些模块已经在 Analyzer 阶段被加载到了 self.ctx.modules
        for module in self.ctx.modules.values() {
            // A. 注入完整 Program AST (用于 Interpreter 顺序执行模块)
            module_programs.insert(module.file_id, module.program.clone());

            // B. 注入 Table AST (碎片化查找)
            for (name, ast_def) in &module.ast_definitions {
                let id = TableId(module.file_id, *name);
                table_defs.insert(id, ast_def.clone());
            }

            // C. 注入 Function AST (碎片化查找)
            for (name, ast_func) in &module.ast_functions {
                func_defs.insert((module.file_id, *name), ast_func.clone());
            }
        }

        // --- 3.2 注入主程序 (Main) 的定义 ---
        // Main 的 Program 目前还在我们手里 (program 变量)，需要封装成 Rc
        let main_program_rc = Rc::new(program.clone()); // 这里的 Clone 无法避免，除非重构 Parser 返回 Rc
        module_programs.insert(file_id, main_program_rc);

        for item in &program.definitions {
            match item {
                TopLevelItem::Table(def) => {
                    let id = TableId(file_id, def.name);
                    table_defs.insert(id, Rc::new(def.clone()));
                }
                TopLevelItem::Function(func_def) => {
                    func_defs.insert((file_id, func_def.name), Rc::new(func_def.clone()));
                }
                _ => {}
            }
        }

        // --- 3.3 初始化 Interpreter ---
        let mut interpreter = Interpreter::new(&mut self.ctx, path, file_id);

        // 填充数据
        interpreter.table_definitions = table_defs;
        interpreter.function_definitions = func_defs;
        interpreter.module_programs = module_programs;

        // --- 3.4 运行 ---
        // eval_program 会先执行 Main 的 TopLevel，然后尝试调用 main() 函数
        match interpreter.eval_program(&program) {
            Ok(v) => Ok(v),
            Err(e) => Err(format!("Runtime Error: {}", e)),
        }
    }

    /// 专门格式化 SemanticError
    fn format_semantic_error(&self, err: &SemanticError) -> String {
        // err.kind 实现了 Display，所以可以直接 to_string
        let msg = format!("Error: {}", err.kind);
        // 使用 err 内部记录的 file_id (可能是 import 的文件)
        self.format_diagnostic(err.file_id, err.span, &msg)
    }

    /// 通用的类似 Rustc 的错误打印机
    /// Error: message
    ///   --> src/main.loom:10:5
    ///    |
    /// 10 |     x = "hello"
    ///    |     ^^^^^^^^^^^
    fn format_diagnostic(&self, file_id: FileId, span: Span, message: &str) -> String {
        let file_name = self
            .ctx
            .source_manager
            .get_file_name(file_id)
            .unwrap_or("<unknown>");

        // 使用 SourceManager 查找行号列号
        if let Some((line, col, line_text)) =
            self.ctx.source_manager.lookup_location(file_id, span.start)
        {
            // 计算高亮箭头的长度
            // 确保不越界，且至少长度为 1
            let highlight_len = if span.end > span.start {
                let max_len = line_text.len().saturating_sub(col - 1);
                std::cmp::min(span.end - span.start, max_len)
            } else {
                1
            };

            // 构建下划线
            let pointer = "^".repeat(highlight_len);
            let padding = " ".repeat(col.saturating_sub(1));

            // 拼装格式
            format!(
                "{}\n  --> {}:{}:{}\n   |\n{:3}| {}\n   | {}{}",
                message,              // Error: Type mismatch...
                file_name,            // src/main.loom
                line,                 // 10
                col,                  // 5
                line,                 // 10
                line_text.trim_end(), // x = "hello"
                padding,              //
                pointer               // ^^^^^^^^^^^
            )
        } else {
            // 如果找不到源码位置 (例如 Span 是 0..0 或文件丢失)
            format!("{} (at {:?})", message, span)
        }
    }
}
