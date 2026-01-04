use lsp_server::{Connection, Message, Request, RequestId, Response};
use lsp_types::{InitializeParams, ServerCapabilities, TextDocumentSyncKind};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

// 引入核心库
use loom::analyzer::{Analyzer, ScopeManager}; // 假设你有这些
use loom::ast::finder::{AstNode, find_node_at_offset};
use loom::ast::*;
use loom::source::{FileId, SourceManager};

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // 1. 初始化日志 (输出到 stderr，因为 stdout 被 LSP 占用了)
    simple_logger::SimpleLogger::new().init().unwrap();
    log::info!("Loom LSP starting...");

    // 2. 建立连接 (基于 stdio)
    let (connection, io_threads) = Connection::stdio();

    // 3. 处理初始化握手 (Initialize)
    // server_capabilities 定义了我们支持什么功能 (高亮、跳转、补全等)
    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        // 告诉 VSCode：文本同步方式是“全量更新” (Full) 还是“增量更新” (Incremental)
        // 简单起见，先用 Full，每次文件变动都发整个文件内容
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::FULL,
        )),
        // 声明支持 "Goto Definition"
        definition_provider: Some(lsp_types::OneOf::Left(true)),

        // 声明支持 "Hover" (悬停提示)
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),

        ..Default::default()
    })
    .unwrap();

    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    log::info!("Loom LSP shutting down");
    Ok(())
}

struct ServerState {
    source_manager: SourceManager,
    // 简单的缓存：FileId -> (AST, AnalyzerResult)
    // 实际项目中可能需要更复杂的增量编译管理
    analysis_cache: HashMap<FileId, (loom::ast::Program, ScopeManager)>,
}

fn main_loop(connection: Connection, params: Value) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();

    let mut state = ServerState {
        source_manager: SourceManager::new(),
        analysis_cache: HashMap::new(),
    };

    log::info!("Loom LSP initialized!");

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }

                match cast::<lsp_types::request::GotoDefinition>(req) {
                    Ok((id, params)) => {
                        let uri = params.text_document_position_params.text_document.uri;
                        let position = params.text_document_position_params.position;

                        // 1. 获取文件 ID
                        // 注意：这里简化了 path 处理，实际需要把 file://URI 转为本地 Path
                        let path = uri.to_file_path().unwrap();
                        // 这里有个假设：文件必须先被 didOpen 加载过
                        // 实际需要处理 load_file 可能失败的情况
                        let file_id = state.source_manager.load_file(&path).unwrap();

                        // 2. 将 Line/Col 转为 Offset
                        let file = state.source_manager.get_file(file_id);
                        let offset =
                            file.offset_at(position.line as usize, position.character as usize);

                        let mut result = None;

                        if let Some(offset) = offset {
                            // 3. 从缓存取出 AST 和 Analyzer
                            if let Some((program, scope_manager)) =
                                state.analysis_cache.get(&file_id)
                            {
                                // 4. 查找节点
                                if let Some(node) = find_node_at_offset(program, offset) {
                                    // 5. 解析定义
                                    if let Some(target_span) =
                                        resolve_definition(node, scope_manager)
                                    {
                                        // 6. 将目标 Span 转回 LSP Location
                                        // 注意：target_span 可能在另一个文件里，这里简化为单文件
                                        let target_file = state.source_manager.get_file(file_id); // 假设定义在同文件
                                        let (start_line, start_col, _) =
                                            target_file.lookup_location(target_span.start);
                                        let (end_line, end_col, _) =
                                            target_file.lookup_location(target_span.end);

                                        result = Some(lsp_types::GotoDefinitionResponse::Scalar(
                                            lsp_types::Location {
                                                uri: uri, // 应该用定义所在文件的 URI
                                                range: lsp_types::Range {
                                                    start: lsp_types::Position::new(
                                                        start_line as u32 - 1,
                                                        start_col as u32 - 1,
                                                    ),
                                                    end: lsp_types::Position::new(
                                                        end_line as u32 - 1,
                                                        end_col as u32 - 1,
                                                    ),
                                                },
                                            },
                                        ));
                                    }
                                }
                            }
                        }

                        let resp = Response::new_ok(id, result);
                        connection.sender.send(Message::Response(resp))?;
                        continue;
                    }
                    Err(req) => { /* ... other requests */ }
                }
            }
            Message::Notification(not) => {
                match not.method.as_str() {
                    "textDocument/didOpen" | "textDocument/didChange" => {
                        // 这里需要解析参数拿到 URI 和 Content
                        // 调用 state.source_manager.add_file(...)
                        // 调用 Parser::parse(...)
                        // 调用 Analyzer::analyze(...)
                        // 更新 state.analysis_cache
                        log::info!("File updated, re-analyzing...");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// 核心逻辑：把 AST 节点和符号表连起来
fn resolve_definition(node: AstNode, scope_manager: &ScopeManager) -> Option<loom::utils::Span> {
    let symbol = match node {
        AstNode::Expression(expr) => match &expr.data {
            ExpressionData::Identifier(sym) => *sym,
            // 还需要处理 FieldAccess (稍微复杂，需要类型推导结果)
            _ => return None,
        },
        AstNode::TypeRef(ty) => match &ty.data {
            ast::TypeRefData::Named(sym) => *sym,
            _ => return None,
        },
    };

    // 在 Scope 中查找
    if let Some(info) = scope_manager.resolve(symbol) {
        // 假设 SymbolInfo 里存了 definition_span
        // 你之前的 SymbolInfo 里好像没有 defined_span？需要加一个！
        return Some(info.defined_span);
    }
    None
}

// 辅助函数：尝试将通用的 Request 转换为具体的 LSP Request 类型
fn cast<R>(req: Request) -> Result<(RequestId, R::Params), Request>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}
