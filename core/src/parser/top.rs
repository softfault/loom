use crate::ast::*;
use crate::parser::{ParseError, ParseResult, Parser};
use crate::token::TokenKind;
use crate::utils::{Span, Symbol};

impl<'a> Parser<'a> {
    /// 解析顶层 Table 定义
    /// 语法: [Name<Generics>: Prototype]
    ///       field = ...
    ///       method = ...
    pub fn parse_table_definition(&mut self) -> ParseResult<TableDefinition> {
        let start_span = self.peek().span;

        // 1. Header: [Name<G>: Proto]
        self.expect(TokenKind::LeftBracket)?;
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);

        let generics = if self.check(TokenKind::LessThan) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        let prototype = if self.match_token(&[TokenKind::Colon]) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::RightBracket)?;

        // Header 后允许换行
        if self.check(TokenKind::Newline) {
            self.advance();
        }

        // 2. Body: 解析 Items
        let mut items = Vec::new();

        loop {
            // A. 跳过空行 (非常重要，否则连续换行会卡死)
            while self.match_token(&[TokenKind::Newline]) {}

            // B. 退出条件
            // 1. 文件结束
            // 2. 遇到下一个 Table 的开始 '[' (因为 Loom 不一定用缩进强制层级，遇到新Header说明当前结束)
            if self.is_at_end() || self.check(TokenKind::LeftBracket) {
                break;
            }

            // C. 错误恢复/检查
            // 如果既不是Identifier，又没结束，说明有垃圾字符
            if !self.check(TokenKind::Identifier) {
                // 这里可以选择直接报错，或者尝试 skip 直到下一行（错误恢复）
                // 简单起见，我们直接报错
                let token = self.peek();
                return Err(crate::parser::ParseError {
                    expected: "Field or Method Name".into(),
                    found: token.kind,
                    span: token.span,
                    message: "Expected a definition inside the table".into(),
                });
            }

            // D. 解析单个 Item
            // 此时 peek 必然是 Identifier，放心交给 parse_table_item
            let item = self.parse_table_item()?;
            items.push(item);
        }

        let end_span = self.previous_span();

        Ok(self.make_node(
            TableDefinitionData {
                name,
                prototype,
                generics,
                items,
            },
            start_span.to(end_span),
        ))
    }

    /// 解析泛型参数列表 <T: Base, U>
    fn parse_generic_params(&mut self) -> ParseResult<Vec<GenericParam>> {
        self.expect(TokenKind::LessThan)?;

        let mut params = Vec::new();

        while !self.check(TokenKind::GreaterThan) && !self.is_at_end() {
            let start_span = self.peek().span;

            // T
            let name_token = self.expect(TokenKind::Identifier)?;
            let name = self.intern_token(name_token);

            // : Constraint (Optional)
            let constraint = if self.match_token(&[TokenKind::Colon]) {
                Some(self.parse_type()?)
            } else {
                None
            };

            let end_span = self.previous_span();
            params.push(self.make_node(
                GenericParamData { name, constraint },
                start_span.to(end_span),
            ));

            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }

        self.expect(TokenKind::GreaterThan)?;
        Ok(params)
    }

    /// 解析 Table 内部条目 (字段 或 方法)
    fn parse_table_item(&mut self) -> ParseResult<TableItem> {
        // 1. 解析名字 (Identifier)
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);
        let start_span = name_token.span;

        // 2. 解析可选的类型标注 (: Type)
        let type_annotation = if self.match_token(&[TokenKind::Colon]) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // 3. 判断是否有赋值号 '='
        // 这里是核心改动：用 match_token 而不是 expect
        if self.match_token(&[TokenKind::Assign]) {
            // === 分支 A: 有等号 (=) ===
            // 可能是字段赋值 (val = 1)
            // 也可能是方法定义 (method = (args) ...)

            // 判别逻辑：如果是 '(' 且后面像参数列表，那就是方法
            let is_method_start = self.check(TokenKind::LeftParen) && self.looks_like_param_list();

            if is_method_start {
                // -> 解析方法
                // 注意：这里要把之前解析的 name 和 span 传进去，或者让 method_def 自己处理
                // 假设 parse_method_definition 负责解析 (args) body
                let method = self.parse_method_definition(name, start_span)?;
                Ok(TableItem::Method(method))
            } else {
                // -> 解析字段 (有初始值)
                let expr = self.parse_expression()?;

                // 允许并吃掉结尾的换行符
                self.match_token(&[TokenKind::Newline]);

                let end_span = expr.span;
                Ok(TableItem::Field(self.make_node(
                    FieldDefinitionData {
                        name,
                        type_annotation,
                        value: Some(expr), // 有值
                    },
                    start_span.to(end_span),
                )))
            }
        } else {
            // === 分支 B: 没有等号 (仅声明) ===
            // 语法: val: T
            // 这种情况下，后面必须紧跟换行符、EOF 或 '}' (如果 Table 用花括号的话)
            // Loom 用缩进/换行分隔，所以这里检查换行

            if self.check(TokenKind::Newline)
                || self.is_at_end()
                || self.check(TokenKind::RightBracket)
            {
                // 吃掉换行 (如果是 EOF 或 ] 则不吃，留给上层处理)
                self.match_token(&[TokenKind::Newline]);

                Ok(TableItem::Field(self.make_node(
                    FieldDefinitionData {
                        name,
                        type_annotation,
                        value: None, // 无值
                    },
                    start_span, // Span 只有名字和类型那么长
                )))
            } else {
                // 既没有等号，也不是合法的结尾，报错
                let found = self.peek();
                Err(crate::parser::ParseError {
                    expected: "'=' or Newline".to_string(),
                    found: found.kind,
                    span: found.span,
                    message:
                        "Field declaration must allow initialization ('=') or end with a newline"
                            .to_string(),
                })
            }
        }
    }

    /// 解析方法定义 (已经消耗了 name 和 =)
    /// 语法: (a: int) int \n Indent ... Dedent
    /// 或者: (a: int) int => expression
    fn parse_method_definition(
        &mut self,
        name: Symbol,
        start_span: Span,
    ) -> ParseResult<MethodDefinition> {
        // 1. 参数列表
        let params = self.parse_param_list()?;

        // 2. 返回类型
        let return_type = if !self.check(TokenKind::Newline)
            && !self.check(TokenKind::Indent)
            && !self.check(TokenKind::FatArrow)
            && !self.check(TokenKind::LeftBrace)
            && !self.check(TokenKind::Equal)
        // 防御性
        {
            Some(self.parse_type()?)
        } else {
            None
        };

        // 3. 方法体 (Body) - 关键修改
        let mut body = None;
        let mut end_span = self.previous_span(); // 默认结束位置在返回类型或参数列表末尾

        if self.match_token(&[TokenKind::FatArrow]) {
            // Case A: 单行模式 => expr
            let expr = self.parse_expression()?;
            end_span = expr.span;
            body = Some(self.make_node(
                BlockData {
                    statements: vec![expr],
                },
                end_span,
            ));
        } else {
            // Case B: 块模式 或 纯签名
            // 允许方法签名后换行
            if self.check(TokenKind::Newline) {
                self.advance();
            }

            // 检查是否有缩进？
            if self.check(TokenKind::Indent) {
                // 有缩进 -> 是具体实现
                let block = self.parse_block()?;
                end_span = block.span;
                body = Some(block);
            } else {
                // 没有缩进 (是 Dedent, Identifier, EOF 等) -> 纯签名 (Abstract)
                // body 保持为 None
            }
        }

        Ok(self.make_node(
            MethodDefinitionData {
                name,
                params,
                return_type,
                body,
            },
            start_span.to(end_span),
        ))
    }

    /// 解析参数列表 (a: int, b: str)
    fn parse_param_list(&mut self) -> ParseResult<Vec<Param>> {
        self.expect(TokenKind::LeftParen)?;
        let mut params = Vec::new();

        while !self.check(TokenKind::RightParen) && !self.is_at_end() {
            let start_span = self.peek().span;

            let name_token = self.expect(TokenKind::Identifier)?;
            let name = self.intern_token(name_token);

            self.expect(TokenKind::Colon)?;
            let type_annotation = self.parse_type()?;

            params.push(self.make_node(
                ParamData {
                    name,
                    type_annotation,
                },
                start_span.to(self.previous_span()),
            ));

            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }

        self.expect(TokenKind::RightParen)?;
        Ok(params)
    }

    /// 解析代码块
    /// Expects: Indent -> Stmts -> Dedent
    pub fn parse_block(&mut self) -> ParseResult<Block> {
        let start_span = self.peek().span;
        self.expect(TokenKind::Indent)?;

        let mut statements = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.is_at_end() {
            // 跳过空行
            if self.match_token(&[TokenKind::Newline]) {
                continue;
            }
            if self.check(TokenKind::Dedent) {
                break;
            }

            // === 核心修改：无关键字变量定义 ===
            // 检查模式: Identifier + Colon (a : ...) -> 认为是显式变量定义
            // 注意：这需要 Lookahead 2 (Peek 0 是 Ident, Peek 1 是 Colon)
            let is_explicit_decl =
                self.check(TokenKind::Identifier) && self.check_nth(1, TokenKind::Colon);

            let stmt = if is_explicit_decl {
                self.parse_variable_definition_without_keyword()?
            } else {
                // 否则，按普通表达式解析
                // 如果是 a = 1，会被解析成 Binary Expression (Assign)
                self.parse_expression()?
            };

            statements.push(stmt);

            // === 3. 语句分隔逻辑 (Core Fix) ===
            // 逻辑：语句之间必须由 Newline 分隔，除非：
            // A. 已经到达了 Block 的末尾 (Dedent)
            // B. 或者是文件末尾 (EOF)
            // C. 或者上一条语句本身就是以 Block 结束的 (previous == Dedent)，类似于 Rust 的 '}'

            if !self.check(TokenKind::Dedent) {
                // 如果当前不是 Dedent (即 Block 还没结束)
                if self.match_token(&[TokenKind::Newline]) {
                    // 情况 1: 普通的换行分隔 ( x=1 \n y=2 ) -> 消耗 Newline
                    continue;
                } else if self.is_at_end() {
                    // 情况 2: 文件结束 -> 允许
                    break;
                } else if self.previous_kind == TokenKind::Dedent {
                    // [逻辑修正]: 情况 3
                    // 上一条语句是 if/while 等，它吃掉了 Dedent。
                    // Dedent 本身就隐含了换行语义，所以这里不需要额外的 Newline。
                    // 直接进入下一次循环，解析下一条语句 (如 SmallSelf)
                    continue;
                } else {
                    // 错误情况: 两个语句挤在一行，且中间没有块结束符
                    let current = self.peek();
                    return Err(ParseError {
                        expected: "Newline or Dedent".to_string(),
                        found: current.kind,
                        span: current.span,
                        message: "Expected a newline or end of block after statement".to_string(),
                    });
                }
            }
        }

        self.expect(TokenKind::Dedent)?;

        Ok(self.make_node(
            BlockData { statements },
            start_span.to(self.previous_span()),
        ))
    }

    /// 新增 helper：解析没有 var/let 的定义
    /// 语法: name : type = value
    fn parse_variable_definition_without_keyword(&mut self) -> ParseResult<Expression> {
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);
        let start_span = name_token.span;

        // 必须有冒号 (因为这是进入此函数的条件)
        self.expect(TokenKind::Colon)?;

        // 解析类型
        let ty = self.parse_type()?;

        // 必须有等号 (Loom 强类型不允许未初始化的变量)
        self.expect(TokenKind::Assign)?;

        let init = self.parse_expression()?;
        let end_span = init.span;

        Ok(self.make_node(
            ExpressionData::VariableDefinition {
                is_mut: true, // 默认可变，或者根据需求
                name,
                ty: Some(ty),
                init: Box::new(init),
            },
            start_span.to(end_span),
        ))
    }

    // --- Lookahead Helpers ---

    /// 简单的 Lookahead 判断是否像参数列表
    /// (a: int) -> 包含冒号，或者是空的 ()
    fn looks_like_param_list(&mut self) -> bool {
        // 前提：当前 peek 是 '('
        if !self.check(TokenKind::LeftParen) {
            return false;
        }

        // Case 1: 空参数列表 ()
        if self.check_nth(1, TokenKind::RightParen) {
            return true;
        }

        // Case 2: (ident : ...)
        if self.check_nth(1, TokenKind::Identifier) {
            if self.check_nth(2, TokenKind::Colon) {
                return true;
            }
        }

        false
    }

    /// 解析 Use 语句
    /// 语法: use [anchor] path
    /// Examples:
    ///   use std.fs       -> Anchor::Root,    ["std", "fs"]
    ///   use .utils       -> Anchor::Current, ["utils"]
    ///   use ..shared.lib -> Anchor::Parent,  ["shared", "lib"]
    pub fn parse_use_statement(&mut self) -> ParseResult<UseStatement> {
        let start_span = self.expect(TokenKind::Use)?.span;

        // 1. 确定锚点 (Anchor)
        let mut anchor = UseAnchor::Root;

        if self.match_token(&[TokenKind::Dot]) {
            anchor = UseAnchor::Current;
            // 比如 use .utils，点后面必须紧跟标识符
        } else if self.match_token(&[TokenKind::DotDot]) {
            anchor = UseAnchor::Parent;
        }

        // 2. 解析路径片段 (Segments)
        let mut path = Vec::new();

        loop {
            // 期望一个标识符
            let segment_token = self.expect(TokenKind::Identifier)?;
            path.push(self.intern_token(segment_token));

            // 如果后面还有点，说明路径继续
            // 注意：这里要区分 use .utils 和 use std.fs
            // 对于 Anchor::Root (std.fs)，第一个标识符后可能有 .
            // 对于 Anchor::Current (.utils)，第一个标识符(.后那个)后可能有 .

            // 只有当下一个是 Dot 时才继续循环
            if self.check(TokenKind::Dot) {
                self.advance(); // consume '.'
                continue;
            } else {
                break;
            }
        }

        // 3. 完整性检查
        // 如果是 . 或 .. 开头，后面必须至少跟一个标识符
        if path.is_empty() {
            let err_span = self.previous_span();
            return Err(ParseError {
                expected: "identifier".into(),
                found: TokenKind::EOF, // 或者是实际的 token
                span: err_span,
                message: "use statement must have a path after anchor".into(),
            });
        }

        // [New] 解析 'as' 别名
        let mut alias = None;
        if self.match_token(&[TokenKind::As]) {
            // 假设你有检查 identifier 内容的方法，或者 'as' 是 Keyword
            let alias_token = self.expect(TokenKind::Identifier)?;
            alias = Some(self.intern_token(alias_token));
        }

        let end_span = self.previous_span();

        Ok(self.make_node(
            UseStatementData {
                anchor,
                path,
                alias,
            },
            start_span.to(end_span),
        ))
    }
}
