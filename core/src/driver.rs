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

    /// 核心编译管线
    fn run_pipeline(&mut self, file_id: FileId, path: PathBuf) -> Result<Value, String> {
        // 从 Context 获取源码引用，避免 clone 整个 String
        let source = self.ctx.source_manager.get_file(file_id).src.as_str();

        // === Step 1: Parsing ===
        let lexer = Lexer::new(source);

        let mut parser = Parser::new(source, lexer, file_id, &mut self.ctx.interner);

        let program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                // Parser Error 处理
                // 假设 e 是 ParseError { span, message, .. }
                let msg = format!("Parse Error: {}", e.message);
                return Err(self.format_diagnostic(file_id, e.span, &msg));
            }
        };

        // 检查 Parser 的非致命错误
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

        // === Step 2: Analysis (Collect -> Resolve -> Check) ===

        // [Key Change] Analyzer::new 现在接收 FileId
        let mut analyzer = Analyzer::new(&mut self.ctx, file_id);

        // Pass 1: Collect
        analyzer.collect_program(&program);

        // Pass 2: Resolve (只有在 Pass 1 没有致命错误时才继续，或者继续跑为了收集更多错误)
        analyzer.resolve_hierarchy();

        // Pass 3: Check
        analyzer.check_program(&program);

        // 统一处理 Analyzer 阶段的所有错误
        if !analyzer.errors.is_empty() {
            let error_msgs: Vec<String> = analyzer
                .errors
                .iter()
                .map(|e| self.format_semantic_error(e))
                .collect();

            return Err(error_msgs.join("\n\n"));
        }

        // === Step 3: Interpretation ===

        // [New] 准备 AST 注册表
        // Interpreter 需要所有 Table 的 AST 才能运行
        let mut table_defs = HashMap::new();

        // 3.1 注入主程序 (Main File) 的定义
        for item in &program.definitions {
            if let TopLevelItem::Table(def) = item {
                // 主程序的定义，使用当前的 file_id
                let id = TableId(file_id, def.name);
                table_defs.insert(id, Rc::new(def.clone()));
            }
        }

        // 3.2 注入所有已加载模块 (Modules) 的定义
        // Analyzer 已经把它们分析好并存在 ctx.modules 里了
        for module in self.ctx.modules.values() {
            for (name, ast_def) in &module.ast_definitions {
                let id = TableId(module.file_id, *name);
                table_defs.insert(id, ast_def.clone());
            }
        }

        // 3.3 初始化 Interpreter 并注入定义
        // 传入 path 和 file_id
        let mut interpreter = Interpreter::new(&mut self.ctx, path, file_id);

        // [关键] 这一步之前漏了，所以找不到 Main
        interpreter.table_definitions = table_defs;

        // 3.4 运行
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
