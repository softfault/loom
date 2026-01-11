use crate::ast::*;
use crate::parser::{ParseError, ParseResult, Parser};
use crate::token::TokenKind;
use crate::utils::{Span, Symbol};

impl<'a> Parser<'a> {
    // ==========================================
    // Case 1: Class Definition
    // 语法:
    // class Dog<T> : Animal
    //     field: int
    //     fn method() ...
    // ==========================================
    pub fn parse_class_definition(&mut self) -> ParseResult<TopLevelItem> {
        let start_span = self.expect(TokenKind::Class)?.span; // 消耗 'class'

        // 1. Name
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);

        // 2. Generics <T>
        let generics = if self.check(TokenKind::LessThan) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // 3. Inheritance (: Parent)
        let prototype = if self.match_token(&[TokenKind::Colon]) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // 4. Body
        // 允许头部后换行
        if self.check(TokenKind::Newline) {
            self.advance();
        }

        // 必须进入缩进块 (或者由上层保证，但通常这里检查比较好)
        // 注意：如果类是空的，可能直接接 EOF 或下一个 definition，这取决于是否强制要求 Body
        // 这里假设类定义必须有缩进块
        let mut items = Vec::new();

        if self.match_token(&[TokenKind::Indent]) {
            loop {
                // 跳过空行
                while self.match_token(&[TokenKind::Newline]) {}

                if self.check(TokenKind::Dedent) || self.is_at_end() {
                    break;
                }

                // 在 Class 内部，可能是 field 定义，也可能是 fn 方法
                let item = self.parse_class_member()?;
                items.push(item);
            }
            self.expect(TokenKind::Dedent)?;
        } else {
            // 允许空类吗？如果不允许，报错；如果允许 (e.g. `class A`), 也可以不报错
            // 这里为了严谨建议要求 Body，或者允许直接换行结束
        }

        let end_span = self.previous_span();

        Ok(TopLevelItem::Table(self.make_node(
            TableDefinitionData {
                name,
                prototype,
                generics,
                items,
            },
            start_span.to(end_span),
        )))
    }

    /// 解析类成员 (Field 或 Method)
    fn parse_class_member(&mut self) -> ParseResult<TableItem> {
        // Case A: 方法 (fn method_name ...)
        if self.check(TokenKind::Fn) {
            let method = self.parse_function_definition_internal(true)?; // true = implies method
            return Ok(TableItem::Method(method));
        }

        // Case B: 字段 (name: Type = val)
        // 必须以 Identifier 开头
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);
        let start_span = name_token.span;

        // 字段必须有类型标注 (Loom 强类型)
        let type_annotation = if self.match_token(&[TokenKind::Colon]) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // 可选赋值
        let value = if self.match_token(&[TokenKind::Assign]) {
            let expr = self.parse_expression()?;
            Some(expr)
        } else {
            None
        };

        // 必须以 Newline 结束 (除非 EOF 或 Dedent)
        if !self.check(TokenKind::Dedent) && !self.is_at_end() {
            self.expect(TokenKind::Newline)?;
        }

        // 检查: 字段至少需要类型或值
        if type_annotation.is_none() && value.is_none() {
            return Err(ParseError {
                expected: ": Type or = Value".into(),
                found: self.peek().kind,
                span: start_span,
                message: "Field declaration requires a type or value".into(),
            });
        }

        let end_span = self.previous_span();
        Ok(TableItem::Field(self.make_node(
            FieldDefinitionData {
                name,
                type_annotation,
                value,
            },
            start_span.to(end_span),
        )))
    }

    // ==========================================
    // Case 2: Function Definition
    // 语法: fn add(a: int) int
    //          return a + b
    // ==========================================

    // 公共入口：顶层调用
    pub fn parse_function_definition(&mut self) -> ParseResult<TopLevelItem> {
        let func = self.parse_function_definition_internal(false)?;
        Ok(TopLevelItem::Function(func))
    }

    // 内部实现：供 TopLevel 和 ClassMethod 复用
    fn parse_function_definition_internal(
        &mut self,
        is_method: bool,
    ) -> ParseResult<MethodDefinition> {
        let start_span = self.expect(TokenKind::Fn)?.span;

        // Name
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);

        // Generics <T>
        let generics = if self.check(TokenKind::LessThan) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };

        // Params (a: int, b: str)
        let params = self.parse_param_list()?;

        // Return Type (可选)
        // 逻辑：如果在 Newline/Indent/BlockStart 之前有东西，那就是返回类型
        // 你的 check 逻辑很棒，复用那个
        let return_type = if !self.check(TokenKind::Newline)
            && !self.check(TokenKind::Indent)
            && !self.check(TokenKind::FatArrow) // 兼容箭头?
            && !self.check(TokenKind::Equal)
            && !self.check(TokenKind::LeftBrace)
        {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Body
        // 支持:
        // 1. => expr (单行)
        // 2. Indent Block (多行)
        // 3. = expr (兼容老语法?) -> 建议 fn 语法下仅支持 => 或 Block

        let mut body = None;
        let mut end_span = self.previous_span();

        // 允许 Header 后换行 (比如 fn foo()\n Indent)
        if self.match_token(&[TokenKind::Newline]) {
            // just consume
        }

        if self.match_token(&[TokenKind::FatArrow]) {
            let expr = self.parse_expression()?;
            end_span = expr.span;
            body = Some(self.make_node(
                BlockData {
                    statements: vec![expr],
                },
                end_span,
            ));
        } else if self.check(TokenKind::Indent) {
            let block = self.parse_block()?;
            end_span = block.span;
            body = Some(block);
        } else {
            // 如果既没有 => 也没有 Indent，对于 fn 来说是语法错误 (除非是 trait 定义，目前 Loom 没有 interface 关键字)
            return Err(ParseError {
                expected: "Function Body (Indent or =>)".into(),
                found: self.peek().kind,
                span: self.peek().span,
                message: "Function must have a body".into(),
            });
        }

        Ok(self.make_node(
            MethodDefinitionData {
                name,
                generics,
                params,
                return_type,
                body,
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

    /// 解析方法定义 (已经消耗了 name 和 =)
    /// 语法: (a: int) int \n Indent ... Dedent
    /// 或者: (a: int) int => expression
    fn parse_method_definition(
        &mut self,
        name: Symbol,
        start_span: Span,
        generics: Vec<GenericParam>, // New Arg
    ) -> ParseResult<MethodDefinition> {
        let params = self.parse_param_list()?;

        let return_type = if !self.check(TokenKind::Newline)
            && !self.check(TokenKind::Indent)
            && !self.check(TokenKind::FatArrow)
            && !self.check(TokenKind::LeftBrace)
            && !self.check(TokenKind::Equal)
        {
            Some(self.parse_type()?)
        } else {
            None
        };

        let mut body = None;
        let mut end_span = self.previous_span();

        if self.match_token(&[TokenKind::FatArrow]) {
            let expr = self.parse_expression()?;
            end_span = expr.span;
            body = Some(self.make_node(
                BlockData {
                    statements: vec![expr],
                },
                end_span,
            ));
        } else {
            if self.check(TokenKind::Newline) {
                self.advance();
            }
            if self.check(TokenKind::Indent) {
                let block = self.parse_block()?;
                end_span = block.span;
                body = Some(block);
            }
        }

        Ok(self.make_node(
            MethodDefinitionData {
                name,
                generics, // Field
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
        if self.check_nth(1, TokenKind::Identifier) && self.check_nth(2, TokenKind::Colon) {
            return true;
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

    /// [New] 解析顶层变量定义
    /// 语法: name (: type)? = value
    /// 或者: name : type
    pub fn parse_top_level_field(&mut self) -> ParseResult<FieldDefinition> {
        // 1. 解析名字
        let name_token = self.expect(TokenKind::Identifier)?;
        let name = self.intern_token(name_token);
        let start_span = name_token.span;

        // 2. 解析可选类型 (: Type)
        let type_annotation = if self.match_token(&[TokenKind::Colon]) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // 3. 解析赋值
        // 顶层变量通常建议初始化，但也允许 `ver: str` 这种纯声明（如果允许的话）
        let value = if self.match_token(&[TokenKind::Assign]) {
            let expr = self.parse_expression()?;
            Some(expr)
        } else {
            None
        };

        // 4. 结尾检查
        // 必须以换行符结束（或者是 EOF）
        // 如果没有值也没有类型，那是没有意义的单独 Identifier，应该在前面就报错
        if type_annotation.is_none() && value.is_none() {
            return Err(ParseError {
                expected: "'=' or ':'".into(),
                found: self.peek().kind,
                span: self.peek().span,
                message: "Top-level variable must have a type annotation or an initial value"
                    .into(),
            });
        }

        self.match_token(&[TokenKind::Newline]);
        let end_span = self.previous_span(); // 这里的 span 计算可能需要根据是否有 value 微调，不过 previous_span 通常够用

        Ok(self.make_node(
            FieldDefinitionData {
                name,
                type_annotation,
                value,
            },
            start_span.to(end_span),
        ))
    }
}
