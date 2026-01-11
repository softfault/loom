use crate::ast::*;
use crate::parser::{ParseError, ParseResult, Parser};
use crate::token::{Token, TokenKind};

impl<'a> Parser<'a> {
    /// 解析类型引用
    /// Examples:
    /// - int
    /// - MyClass
    /// - lib.MyClass  (New!)
    /// - List<String>
    /// - { name: str, age: int }
    pub fn parse_type(&mut self) -> ParseResult<TypeRef> {
        let start_span = self.peek().span;

        // [NEW] 0. 数组类型 (Array Type): [int] or [[str]]
        // 递归解析，天然支持多维数组
        if self.check(TokenKind::LeftBracket) {
            self.advance(); // consume '['

            // 递归解析内部类型
            let inner_type = self.parse_type()?;

            // 必须以 ']' 结尾
            self.expect(TokenKind::RightBracket)?;

            let end_span = self.previous_span();
            return Ok(self.make_node(
                TypeRefData::Array(Box::new(inner_type)),
                start_span.to(end_span),
            ));
        }

        // 1. 结构化类型 (Structural Type): { name: str }
        if self.check(TokenKind::LeftBrace) {
            return self.parse_structural_type();
        }

        // 2. 基础类型关键字 (Base Types)
        let toke = self.peek();
        if let Some(type_name) = self.get_basic_type_name(toke.kind) {
            self.advance(); // consume keyword
            let name_sym = self.interner.intern(type_name);
            return Ok(self.make_node(TypeRefData::Named(name_sym), start_span));
        }

        // 3. 具名类型、模块成员或泛型 (Identifier start)
        if self.check(TokenKind::Identifier) {
            let name_token = self.advance(); // 消费第一个标识符 (e.g. "int" 或 "lib")
            let name = self.intern_token(name_token);

            // [New] Case A: 模块成员访问 (lib.Animal)
            // 检查后面是否紧跟 '.'
            if self.check(TokenKind::Dot) {
                self.advance(); // 消费 '.'

                // 我们期望下一个 token 必须是 Identifier
                if self.check(TokenKind::Identifier) {
                    let member_token = self.advance(); // 拿到 token
                    let member_name = self.intern_token(member_token);

                    // 构造 Member 节点
                    let end_span = member_token.span;
                    return Ok(self.make_node(
                        TypeRefData::Member {
                            module: name,
                            member: member_name,
                        },
                        start_span.to(end_span),
                    ));
                } else {
                    // 如果不是 Identifier，构建并抛出 ParseError
                    let current = self.peek();
                    return Err(ParseError {
                        expected: "Identifier".to_string(),
                        found: current.kind,
                        span: current.span,
                        message: "Expected type name after '.'".to_string(),
                    });
                }
            }

            // [Existing] Case B: 泛型实例化 (List<int>)
            // 检查后面是否紧跟 '<'
            if self.check(TokenKind::LessThan) {
                let args = self.parse_type_arguments()?;
                let end_span = self.previous_span(); // '>' 的位置

                return Ok(self.make_node(
                    TypeRefData::GenericInstance { base: name, args },
                    start_span.to(end_span),
                ));
            }

            // [Existing] Case C: 普通具名类型 (MyClass)
            return Ok(self.make_node(TypeRefData::Named(name), start_span));
        }

        // 错误处理
        let current = self.peek();
        Err(ParseError {
            expected: "Type".to_string(),
            found: current.kind,
            span: current.span,
            message: "Expected a type (e.g. 'int', 'String', 'lib.Type', '{...}')".to_string(),
        })
    }

    /// 解析泛型类型参数列表 (Type Arguments)
    /// 语法: <Type, Type>
    /// 用于: List<int>, Map<str, int>
    fn parse_type_arguments(&mut self) -> ParseResult<Vec<TypeRef>> {
        self.expect(TokenKind::LessThan)?;

        let mut args = Vec::new();

        // 循环解析类型，直到遇到 '>'
        while !self.check(TokenKind::GreaterThan) && !self.is_at_end() {
            let ty = self.parse_type()?;
            args.push(ty);

            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }

        self.expect(TokenKind::GreaterThan)?;
        Ok(args)
    }

    /// 解析结构化类型 (Anonymous Structural Type)
    /// 语法: { field1: Type, field2: Type }
    fn parse_structural_type(&mut self) -> ParseResult<TypeRef> {
        let start_span = self.peek().span;
        self.expect(TokenKind::LeftBrace)?;

        let mut fields = Vec::new();

        while !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            // 解析字段定义: name: Type
            let field_start = self.peek().span;
            let name_token = self.expect(TokenKind::Identifier)?;
            let name = self.intern_token(name_token);

            self.expect(TokenKind::Colon)?;

            let type_annotation = self.parse_type()?;
            let field_end = type_annotation.span;

            // 复用 AST 中的 Param 结构来存储结构化类型的字段
            fields.push(self.make_node(
                ParamData {
                    name,
                    type_annotation,
                },
                field_start.to(field_end),
            ));

            // 允许逗号分隔
            if !self.match_token(&[TokenKind::Comma]) {
                break;
            }
        }

        self.expect(TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(self.make_node(TypeRefData::Structural(fields), start_span.to(end_span)))
    }

    /// 辅助函数：将 TokenKind 映射为基础类型的字符串表示
    /// 这允许我们在 AST 中把 'int' 关键字当作名为 "int" 的类型处理，简化后端逻辑
    fn get_basic_type_name(&self, kind: TokenKind) -> Option<&'static str> {
        match kind {
            TokenKind::IntType => Some("int"),
            TokenKind::FloatType => Some("float"),
            TokenKind::BoolType => Some("bool"),
            TokenKind::StrType => Some("str"),
            TokenKind::AnyType => Some("any"),
            TokenKind::Nil => Some("nil"), // Nil 也可以作为一种类型
            _ => None,
        }
    }
}
