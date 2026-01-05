use super::SymbolKind;
use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::path::resolve_module_path;
use crate::analyzer::{
    Analyzer, FieldInfo, FunctionSignature, MethodInfo, ModuleInfo, TableId, TableInfo, Type,
};
use crate::ast::*;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::source::FileId;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

impl<'a> Analyzer<'a> {
    pub fn collect_program(&mut self, program: &Program) {
        // 第一步：手动开启最顶层的“全局作用域”
        self.scopes.enter_scope();

        // 第二步：注入内置函数
        self.register_builtins();

        // 第三步：收集用户定义
        for item in &program.definitions {
            match item {
                TopLevelItem::Table(table_def) => self.collect_table_definition(table_def),
                TopLevelItem::Use(use_stmt) => self.collect_use_statement(use_stmt),
            }
        }
    }

    fn collect_table_definition(&mut self, def: &TableDefinition) {
        let name = def.name;
        let id = self.current_file_id;

        let table_id = TableId(id, name);

        // 1. 注册全局符号
        if let Err(_) = self.scopes.define(
            name,
            Type::Table(table_id),
            SymbolKind::Table,
            def.span,
            id,
            false,
        ) {
            let name_str = self.ctx.resolve_symbol(name).to_string();
            self.report(def.span, SemanticErrorKind::DuplicateDefinition(name_str));
            return;
        }

        // 2. 解析泛型参数定义 <T, U>
        let mut generic_params = Vec::new();
        let mut local_generics_scope = HashSet::new();

        for g in &def.generics {
            if local_generics_scope.contains(&g.name) {
                let g_name = self.ctx.resolve_symbol(g.name).to_string();
                self.report(g.span, SemanticErrorKind::DuplicateDefinition(g_name));
            } else {
                generic_params.push(g.name);
                local_generics_scope.insert(g.name);
            }
        }

        // 3. 解析继承关系
        let parent = if let Some(ref proto_type) = def.prototype {
            Some(self.resolve_ast_type(proto_type, &local_generics_scope))
        } else {
            None
        };

        // 4. 解析 Fields 和 Methods
        let mut fields = HashMap::new();
        let mut methods = HashMap::new();

        for item in &def.items {
            match item {
                TableItem::Field(field) => {
                    let ty = if let Some(ref type_ref) = field.type_annotation {
                        self.resolve_ast_type(type_ref, &local_generics_scope)
                    } else {
                        Type::Infer
                    };

                    let field_info = FieldInfo {
                        ty,
                        span: field.span,
                    };

                    if fields.insert(field.name, field_info).is_some() {
                        let f_name = self.ctx.resolve_symbol(field.name).to_string();
                        self.report(field.span, SemanticErrorKind::DuplicateDefinition(f_name));
                    }
                }
                TableItem::Method(method) => {
                    let sig = self.collect_method_signature(method, &local_generics_scope);
                    let method_info = MethodInfo {
                        signature: sig,
                        span: method.span,
                    };
                    if methods.insert(method.name, method_info).is_some() {
                        let m_name = self.ctx.resolve_symbol(method.name).to_string();
                        self.report(method.span, SemanticErrorKind::DuplicateDefinition(m_name));
                    }
                }
            }
        }

        // 5. 存入 Analyzer Tables
        let info = TableInfo {
            name,
            file_id: id,
            parent,
            generic_params,
            fields,
            methods,
            defined_span: def.span,
        };

        self.tables.insert(name, info);
    }

    fn collect_method_signature(
        &self,
        method: &MethodDefinition,
        valid_generics: &HashSet<crate::utils::Symbol>,
    ) -> FunctionSignature {
        let mut params = Vec::new();
        for param in &method.params {
            let p_ty = self.resolve_ast_type(&param.type_annotation, valid_generics);
            params.push((param.name, p_ty));
        }

        let ret = if let Some(ref ret_ty) = method.return_type {
            self.resolve_ast_type(ret_ty, valid_generics)
        } else {
            Type::Unit
        };

        let is_abstract = method.body.is_none();

        FunctionSignature {
            params,
            ret,
            is_abstract,
        }
    }

    /// 处理 use 语句
    pub fn collect_use_statement(&mut self, stmt: &UseStatement) {
        // 1. 获取模块名和 import 名称
        let path_segments: Vec<String> = stmt
            .path
            .iter()
            .map(|s| self.ctx.resolve_symbol(*s).to_string())
            .collect();

        let module_name_sym = *stmt.path.last().unwrap();
        let import_name = stmt.alias.unwrap_or(module_name_sym);

        // 2. 路径解析
        let current_dir = self
            .current_file_path
            .parent()
            .unwrap_or(&self.ctx.root_dir);

        let target_path_raw =
            match resolve_module_path(self.ctx, &stmt.anchor, &path_segments, current_dir) {
                Some(p) => p,
                None => {
                    self.report(
                        stmt.span,
                        SemanticErrorKind::ModuleNotFound(path_segments.join(".")),
                    );
                    return;
                }
            };

        // 3. 转为绝对路径
        let abs_path = match target_path_raw.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                self.report(
                    stmt.span,
                    SemanticErrorKind::InvalidModulePath(format!("{:?}", target_path_raw)),
                );
                return;
            }
        };

        // 4. 检查是否需要加载
        if !self.ctx.modules.contains_key(&abs_path) {
            // --- Cache Miss: 需要加载 ---

            // 4.1 循环依赖检测
            if self.ctx.loading_stack.contains(&abs_path) {
                self.report(
                    stmt.span,
                    SemanticErrorKind::CircularDependency(format!("{:?}", abs_path)),
                );
                return;
            }

            // 4.2 使用 SourceManager 加载文件
            // 注意：这里 load_file 内部处理了 canonicalize 和去重，所以传入 abs_path 是安全的
            let file_id = match self.ctx.source_manager.load_file(&abs_path) {
                Ok(id) => id,
                Err(e) => {
                    self.report(stmt.span, SemanticErrorKind::FileIOError(e.to_string()));
                    return;
                }
            };

            // 4.3 启动子分析器
            self.ctx.loading_stack.insert(abs_path.clone());

            // 使用 file_id 进行分析，不需要再读一次文件
            let module_info = self.analyze_module_file(file_id, abs_path.clone());

            self.ctx.loading_stack.remove(&abs_path);

            // 4.4 写入全局缓存
            if let Some(info) = module_info {
                self.ctx.modules.insert(abs_path.clone(), info);
            }
        }

        // 5. 将模块注册到当前作用域
        if self.ctx.modules.contains_key(&abs_path) {
            // 我们在当前文件定义了一个变量 (模块别名)
            // 当用户点击这个别名时，LSP 会跳转到这条 use 语句
            if let Err(_) = self.scopes.define(
                import_name,
                Type::Module(abs_path), // 类型指向目标模块路径
                SymbolKind::Variable,   // 这里视作一个变量 (或者你可以加一个 SymbolKind::Module)
                stmt.span,              // <--- 1. 定义位置：整条 use 语句
                self.current_file_id,   // <--- 2. 定义文件：当前分析的文件
                false,
            ) {
                let name = self.ctx.resolve_symbol(import_name).to_string();
                self.report(stmt.span, SemanticErrorKind::DuplicateDefinition(name));
            }
        }
    }

    /// 辅助：加载并分析一个新文件
    fn analyze_module_file(&mut self, file_id: FileId, path: PathBuf) -> Option<ModuleInfo> {
        // 0. 从 SourceManager 获取源码引用
        let source_text = self.ctx.source_manager.get_file(file_id).src.as_str();

        // 1. Parsing
        let lexer = Lexer::new(source_text);
        let mut parser = Parser::new(source_text, lexer, file_id, &mut self.ctx.interner);

        let program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                // 将 Parser 的错误桥接到 SemanticError
                self.report(e.span, SemanticErrorKind::ModuleParseError(e.message));
                return None;
            }
        };

        // 2. Create Sub-Analyzer
        // 按照你之前的同意，Analyzer::new 接收 FileId
        let mut sub_analyzer = Analyzer::new(self.ctx, file_id);

        // 3. Collect & Resolve
        sub_analyzer.collect_program(&program);

        if !sub_analyzer.errors.is_empty() {
            self.errors.extend(sub_analyzer.errors);
            return None;
        }

        sub_analyzer.resolve_hierarchy();

        if !sub_analyzer.errors.is_empty() {
            self.errors.extend(sub_analyzer.errors);
            return None;
        }

        // [New] 收集该模块所有的 Table AST
        let mut ast_defs = HashMap::new();
        for item in &program.definitions {
            if let TopLevelItem::Table(def) = item {
                ast_defs.insert(def.name, std::rc::Rc::new(def.clone()));
            }
        }

        Some(ModuleInfo {
            file_id,
            file_path: path,
            exports: sub_analyzer.tables,
            ast_definitions: ast_defs, // [New]
        })
    }

    pub fn resolve_ast_type(
        &self,
        type_ref: &TypeRef,
        valid_generics: &HashSet<crate::utils::Symbol>,
    ) -> Type {
        match &type_ref.data {
            TypeRefData::Named(sym) => {
                // 1. 检查是否是泛型参数 (T)
                if valid_generics.contains(sym) {
                    return Type::GenericParam(*sym);
                }

                // 2. 检查是否是内置基础类型
                let name_str = self.ctx.resolve_symbol(*sym);
                match name_str {
                    "int" => Type::Int,
                    "float" => Type::Float,
                    "bool" => Type::Bool,
                    "str" => Type::Str,
                    "nil" => Type::Nil,
                    "any" => Type::Error, // 暂定为 Error
                    _ => {
                        // [Fix 1] scope.resolve 返回的是 SymbolInfo，不是 Type
                        // 我们需要访问 info.ty
                        if let Some(info) = self.scopes.resolve(*sym) {
                            // 如果已经是 Table 类型（比如 import 进来的），直接复用
                            // 这样能保留它携带的原始 FileId
                            if let Type::Table(_) = &info.ty {
                                return info.ty.clone();
                            }
                        }

                        // [Fix 2] 如果没找到，或者不是 Table，默认为当前文件定义的 Table
                        // 使用 TableId(FileId, Symbol) 构造
                        Type::Table(TableId(self.current_file_id, *sym))
                    }
                }
            }
            TypeRefData::GenericInstance { base, args } => {
                let resolved_args = args
                    .iter()
                    .map(|a| self.resolve_ast_type(a, valid_generics))
                    .collect();

                let base_id = if let Some(info) = self.scopes.resolve(*base) {
                    if let Type::Table(id) = &info.ty {
                        *id
                    } else {
                        TableId(self.current_file_id, *base)
                    }
                } else {
                    // 没 resolve 到，认为是当前文件定义的
                    TableId(self.current_file_id, *base)
                };

                Type::GenericInstance {
                    base: base_id,
                    args: resolved_args,
                }
            }
            TypeRefData::Structural(_) => Type::Infer,
        }
    }
}
