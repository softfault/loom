use super::SymbolKind;
use crate::analyzer::errors::SemanticErrorKind;
use crate::analyzer::path::resolve_module_path;
use crate::analyzer::{
    Analyzer, FieldInfo, FunctionInfo, FunctionSignature, GlobalVarInfo, MethodInfo, ModuleInfo,
    TableId, TableInfo, Type,
};
use crate::ast::*;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::source::FileId;
use crate::utils::Symbol;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

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

                // [New] 处理顶层函数
                TopLevelItem::Function(func_def) => self.collect_top_level_function(func_def),

                // [New] 处理顶层变量
                TopLevelItem::Field(field_def) => self.collect_top_level_field(field_def),
            }
        }
    }

    // 收集顶层函数
    fn collect_top_level_function(&mut self, def: &MethodDefinition) {
        let name = def.name;
        let id = self.current_file_id;

        // 1. 收集泛型参数名 (为了存入 FunctionInfo)
        // 注意：这里只做收集，不做查重检查，查重交给 collect_method_signature 去做
        let mut generic_params = Vec::new();
        for g in &def.generics {
            generic_params.push(g.name);
        }

        // 2. 准备“外部”泛型作用域
        // 对于顶层函数，外部没有泛型（没有 Class<T>），所以是空的
        let parent_scope = HashSet::new();

        // 3. 解析签名
        // collect_method_signature 会自动处理 def.generics，把它加入到 parent_scope 中用于解析参数
        // 所以我们传入空的 parent_scope 即可
        let signature = self.collect_method_signature(def, &parent_scope);

        // 4. 构造函数类型 (Type::Function)
        // 记得带上 generic_params
        let func_type = Type::Function {
            generic_params: generic_params.clone(),
            params: signature.params.iter().map(|(_, t)| t.clone()).collect(),
            ret: Box::new(signature.ret.clone()),
        };

        // 5. 注册符号
        if let Err(_) =
            self.scopes
                .define(name, func_type, SymbolKind::Function, def.span, id, false)
        {
            let name_str = self.ctx.resolve_symbol(name).to_string();
            self.report(def.span, SemanticErrorKind::DuplicateDefinition(name_str));
            return;
        }

        // 6. 存入 Analyzer 表
        let info = FunctionInfo {
            name,
            generic_params,
            signature,
            span: def.span,
            file_id: id,
        };

        self.functions.insert(name, info);
    }

    // [New] 收集顶层变量
    fn collect_top_level_field(&mut self, def: &FieldDefinition) {
        // 类似于 Table 里的 field，但没有 generic context
        let empty_generics = HashSet::new();

        let ty = if let Some(ref type_ref) = def.type_annotation {
            self.resolve_ast_type(type_ref, &empty_generics)
        } else {
            Type::Infer // 需要类型推导
        };

        // [Fix] 注册并处理错误
        if let Err(_) = self.scopes.define(
            def.name,
            ty.clone(),
            SymbolKind::Variable,
            def.span,
            self.current_file_id,
            false,
        ) {
            // 从 interner 解析出字符串名称用于报错
            let name_str = self.ctx.resolve_symbol(def.name).to_string();
            self.report(def.span, SemanticErrorKind::DuplicateDefinition(name_str));
            // 即使报错，也可以选择继续往后跑，或者直接 return，取决于你是否想做错误恢复
            // 这里通常 return 避免后续逻辑产生更多混乱的错误
            return;
        }

        let info = GlobalVarInfo {
            name: def.name,
            ty,
            span: def.span,
            file_id: self.current_file_id,
            is_const: false,
        };

        self.globals.insert(def.name, info); // 假设你在 Analyzer 里加了这个字段
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
                    let method_generics: Vec<Symbol> =
                        method.generics.iter().map(|g| g.name).collect();
                    let method_info = MethodInfo {
                        generic_params: method_generics,
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

        self.tables.insert(table_id, info);
    }

    fn collect_method_signature(
        &mut self,
        method: &MethodDefinition,
        class_generics: &HashSet<crate::utils::Symbol>, // 来自类的泛型
    ) -> FunctionSignature {
        // 1. 合并泛型作用域
        // 方法内部可见的泛型 = 类泛型 T + 方法泛型 U
        let mut valid_generics = class_generics.clone();

        for g in &method.generics {
            // [Fix] 检查重复或遮蔽
            if valid_generics.contains(&g.name) {
                let g_name = self.ctx.resolve_symbol(g.name).to_string();

                // 如果是遮蔽了类的泛型，报 Shadowing 错
                // 如果是方法参数列表里自己重复了 <T, T>，其实也是一种 Shadowing/Duplicate
                // 这里统一用 GenericShadowing 比较准确
                self.report(g.span, SemanticErrorKind::GenericShadowing(g_name));
            } else {
                valid_generics.insert(g.name);
            }
        }

        // 2. 解析参数 (使用合并后的 scope)
        let mut params = Vec::new();
        for param in &method.params {
            let p_ty = self.resolve_ast_type(&param.type_annotation, &valid_generics);
            params.push((param.name, p_ty));
        }

        // 3. 解析返回值
        let ret = if let Some(ref ret_ty) = method.return_type {
            self.resolve_ast_type(ret_ty, &valid_generics)
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

        // 2. 路径解析 & 转绝对路径
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

        // 3. [关键步骤] 获取 FileId
        // 无论是否已经分析过，我们都需要 ID 来构造 Type::Module
        // SourceManager.load_file 内部有缓存去重机制，这里调用是安全的
        let file_id = match self.ctx.source_manager.load_file(&abs_path) {
            Ok(id) => id,
            Err(e) => {
                self.report(stmt.span, SemanticErrorKind::FileIOError(e.to_string()));
                return;
            }
        };

        // 4. 检查是否需要分析 (Cache Miss)
        // 我们使用 abs_path 作为模块缓存的 Key (Analyzer 阶段)
        if !self.ctx.modules.contains_key(&abs_path) {
            // 4.1 循环依赖检测
            if self.ctx.loading_stack.contains(&abs_path) {
                self.report(
                    stmt.span,
                    SemanticErrorKind::CircularDependency(format!("{:?}", abs_path)),
                );
                // 发生循环依赖时，为了防止后续崩溃，可以注册一个 Error 类型，或者直接返回
                return;
            }

            // 4.2 启动分析
            self.ctx.loading_stack.insert(abs_path.clone());

            // 递归分析
            let module_info = self.analyze_module_file(file_id, abs_path.clone());

            self.ctx.loading_stack.remove(&abs_path);

            // 4.3 写入全局缓存
            if let Some(info) = module_info {
                self.ctx.modules.insert(abs_path.clone(), info);
            }
        }

        // 5. 将模块注册到当前作用域
        // [Key Fix] 使用 file_id 构造 Type::Module
        if let Err(_) = self.scopes.define(
            import_name,
            Type::Module(file_id), // <--- 这里现在正确使用了 FileId
            SymbolKind::Variable,  // 模块在当前作用域表现为一个变量
            stmt.span,
            self.current_file_id,
            false, // 模块引用通常不可变
        ) {
            let name = self.ctx.resolve_symbol(import_name).to_string();
            self.report(stmt.span, SemanticErrorKind::DuplicateDefinition(name));
        }
    }

    fn analyze_module_file(&mut self, file_id: FileId, path: PathBuf) -> Option<ModuleInfo> {
        let source_text = self.ctx.source_manager.get_file(file_id).src.as_str();

        let lexer = Lexer::new(source_text);
        let mut parser = Parser::new(source_text, lexer, file_id, &mut self.ctx.interner);

        let program = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                self.report(e.span, SemanticErrorKind::ModuleParseError(e.message));
                return None;
            }
        };

        // 隔离环境分析子模块
        let mut sub_analyzer = Analyzer::new(self.ctx, file_id);

        // 收集符号 (会填充 sub_analyzer.functions / tables / globals)
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

        // --- 收集 AST (供解释器使用) ---
        let mut ast_defs = HashMap::new();
        let mut ast_funcs = HashMap::new(); // [New]

        for item in &program.definitions {
            match item {
                TopLevelItem::Table(def) => {
                    ast_defs.insert(def.name, std::rc::Rc::new(def.clone()));
                }
                TopLevelItem::Function(func) => {
                    // [New] 必须保存顶层函数的 AST，否则解释器无法执行它
                    ast_funcs.insert(func.name, std::rc::Rc::new(func.clone()));
                }
                _ => {} // 变量和Use语句不需要AST
            }
        }

        // --- 构建 ModuleInfo ---
        Some(ModuleInfo {
            file_id,
            file_path: path,

            // 导出元数据 (从子分析器里偷出来)
            tables: sub_analyzer.tables,
            functions: sub_analyzer.functions,
            globals: sub_analyzer.globals,

            // 导出 AST
            ast_definitions: ast_defs,
            ast_functions: ast_funcs,
            program: Rc::new(program),
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
            // [New] 处理模块成员类型 (lib.Animal)
            TypeRefData::Member { module, member } => {
                // 1. 在当前作用域查找模块变量 (例如 "animal_lib")
                if let Some(info) = self.scopes.resolve(*module) {
                    // 2. 检查它是不是一个 Module 类型
                    // 注意：Analyzer 的 Type enum 中需要有 Module(FileId) 变体
                    if let Type::Module(file_id) = info.ty {
                        // 3. 构造指向该模块的 TableId
                        return Type::Table(TableId(file_id, *member));
                    }
                }

                // 如果找不到模块，或者找到的不是模块，这是语义错误
                // 但在这个阶段我们只负责转换类型，返回 Error 让 Check 阶段去报错
                Type::Error
            }
        }
    }
}
