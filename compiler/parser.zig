const std = @import("std");
const Token = @import("token.zig").Token;
const TokenType = @import("token.zig").TokenType;
const TokenStream = @import("token_stream.zig").TokenStream;
const StringInterner = @import("utils.zig").StringInterner;
const SymbolId = @import("utils.zig").SymbolId;
const ast = @import("ast.zig");
const Span = @import("utils.zig").Span;
const Context = @import("context.zig").Context;

/// 显式定义的错误集合
/// 用于打破递归函数的错误推导循环 (Error Set Inference Loop)
pub const ParseError = error{
    OutOfMemory, // 用于所有内存分配 (allocator.create/alloc)
    ParseError, // 核心错误：用于 synchronize 时中断控制流
    UnexpectedToken, // 用于 expect() 失败
    InvalidEscapeSequence, // 用于 parseStringLiteral
    UnterminatedUnicodeEscape, // 用于 parseStringLiteral
    InvalidUnicodeScalar, // 用于 parseStringLiteral
    ExpectedSemicolon, // 用于 parseStatement
};

pub const ParseErrorTag = enum {
    UnexpectedToken,
    ExpectedIdentifier,
    ExpectedExpression,
    UnterminatedString,
    InvalidEscapeSequence,
    ExpectedSemicolon,
    ExpectedType,
    // ...

    /// 将 Tag 转换为人类可读的格式化字符串模板
    pub fn message(self: ParseErrorTag, allocator: std.mem.Allocator, token_text: []const u8) ![]u8 {
        return switch (self) {
            // 需要参数的情况：
            .UnexpectedToken => std.fmt.allocPrint(allocator, "Unexpected token: '{s}'", .{token_text}),
            .ExpectedIdentifier => std.fmt.allocPrint(allocator, "Expected identifier, found '{s}'", .{token_text}),

            // 不需要参数的情况（忽略 token_text，避免 "unused argument" 报错）：
            .ExpectedExpression => allocator.dupe(u8, "Expected expression"),
            .UnterminatedString => allocator.dupe(u8, "Unterminated string literal"),
            .InvalidEscapeSequence => allocator.dupe(u8, "Invalid escape sequence"),
            .ExpectedSemicolon => allocator.dupe(u8, "Expect semicolon"),
            .ExpectedType => allocator.dupe(u8, "Expected a Type"),
        };
    }
};

pub const Parser = struct {
    /// 1. AST 专用 Arena
    /// 解析产生的所有 AST 节点都住在这里
    ast_arena: std.heap.ArenaAllocator,

    /// 2. 上下文引用
    context: *Context,

    /// 3. 核心组件引用
    stream: TokenStream,
    source: []const u8, // 用于从 Span 切片获取原始字符串

    /// 4. 状态标记
    panic_mode: bool = false, // 用于错误恢复 (Synchronization)

    pub fn init(
        allocator: std.mem.Allocator, // 传入通用分配器 (如 GPA)
        stream: TokenStream,
        context: *Context,
        source: []const u8,
    ) Parser {
        return .{
            .ast_arena = std.heap.ArenaAllocator.init(allocator),
            .stream = stream,
            .context = context,
            .source = source,
        };
    }

    pub fn deinit(self: *Parser) void {
        self.ast_arena.deinit();
    }

    // ==========================================
    // Core Tools: AST Node Allocation
    // ==========================================

    /// 在 Arena 上创建一个节点并返回指针
    pub fn create(self: *Parser, comptime T: type, data: T) !*T {
        const ptr = try self.ast_arena.allocator().create(T);
        ptr.* = data;
        return ptr;
    }

    /// 在 Arena 上分配一个切片 (比如参数列表)
    pub fn allocList(self: *Parser, comptime T: type, capacity: usize) ![]T {
        return self.ast_arena.allocator().alloc(T, capacity);
    }

    // ==========================================
    // Core Tools: Token Consumption
    // ==========================================

    fn peek(self: *Parser) Token {
        return self.stream.peek(0);
    }

    fn advance(self: *Parser) Token {
        return self.stream.advance();
    }

    fn check(self: *Parser, tag: TokenType) bool {
        return self.peek().tag == tag;
    }

    fn match(self: *Parser, tags: []const TokenType) bool {
        for (tags) |tag| {
            if (self.check(tag)) {
                _ = self.advance();
                return true;
            }
        }
        return false;
    }

    /// 消费一个 Token，如果类型不对则报错 (Sync 入口)
    fn expect(self: *Parser, tag: TokenType) !Token {
        if (self.check(tag)) {
            return self.advance();
        }
        try self.errorAtCurrent(.UnexpectedToken);
        return error.ParseError;
    }

    // ==========================================
    // Integration: String Interner & Unescape
    // ==========================================

    /// 将 Token 的原始文本 Intern 到符号表中
    fn internToken(self: *Parser, token: Token) !SymbolId {
        const text = token.span.slice(self.source);
        return self.context.intern(text);
    }

    /// 处理字符串字面量：去引号 -> 转义 -> Intern
    /// 例如: "hello\n" (raw 9 bytes) -> hello\n (actual 6 bytes) -> SymbolId
    fn parseStringLiteral(self: *Parser, token: Token) !SymbolId {
        const raw = token.span.slice(self.source);

        // 1. 去掉引号
        if (raw.len < 2) return error.ParseError;
        const inner = raw[1 .. raw.len - 1];

        // 2. 转义处理
        // 利用 ast_arena 作为临时缓冲区分配转义后的字符串
        const unescaped = try self.unescapeString(inner);

        // 3. Intern
        return self.context.intern(unescaped);
    }

    fn unescapeString(self: *Parser, input: []const u8) ![]const u8 {
        // 悲观估计：转义后的长度绝对不会超过原长度（\n 是 2 变 1，\u{...} 是多变少）
        // 所以直接用 input.len 分配是安全的，不需要担心 overflow
        const buffer = try self.ast_arena.allocator().alloc(u8, input.len);
        var index: usize = 0; // buffer 的写入游标
        var i: usize = 0; // input 的读取游标

        while (i < input.len) {
            if (input[i] == '\\' and i + 1 < input.len) {
                const c = input[i + 1];

                // 处理普通单字符转义
                switch (c) {
                    'n' => {
                        buffer[index] = '\n';
                        index += 1;
                        i += 2;
                    },
                    'r' => {
                        buffer[index] = '\r';
                        index += 1;
                        i += 2;
                    },
                    't' => {
                        buffer[index] = '\t';
                        index += 1;
                        i += 2;
                    },
                    '\\' => {
                        buffer[index] = '\\';
                        index += 1;
                        i += 2;
                    },
                    '\'' => {
                        buffer[index] = '\'';
                        index += 1;
                        i += 2;
                    },
                    '\"' => {
                        buffer[index] = '\"';
                        index += 1;
                        i += 2;
                    },
                    '0' => {
                        buffer[index] = 0;
                        index += 1;
                        i += 2;
                    },

                    // 处理 \xNN (十六进制字节)
                    'x' => {
                        if (i + 3 >= input.len) return error.InvalidEscapeSequence;
                        const hex_slice = input[i + 2 .. i + 4];
                        const byte_val = std.fmt.parseInt(u8, hex_slice, 16) catch return error.InvalidEscapeSequence;
                        buffer[index] = byte_val;
                        index += 1;
                        i += 4; // 跳过 \xNN
                    },

                    // 处理 \u{XXXX} (Unicode 代码点)
                    'u' => {
                        // 1. 检查基本结构 \u{
                        if (i + 2 >= input.len or input[i + 2] != '{') {
                            // 如果你不想支持无括号的 \uXXXX，就直接报错
                            return error.InvalidEscapeSequence;
                        }

                        // 2. 寻找闭合的 }
                        const start_hex = i + 3;
                        var end_hex = start_hex;
                        while (end_hex < input.len and input[end_hex] != '}') {
                            end_hex += 1;
                        }

                        if (end_hex >= input.len) return error.UnterminatedUnicodeEscape;

                        // 3. 解析十六进制
                        const hex_slice = input[start_hex..end_hex];
                        // Unicode 最大是 u21
                        const codepoint = std.fmt.parseInt(u21, hex_slice, 16) catch return error.InvalidUnicodeScalar;

                        // 4. 将代码点编码为 UTF-8 字节序列写入 buffer
                        // std.unicode.utf8Encode 需要一个足够大的 slice 来写入
                        // 这里给它 buffer 剩下的空间
                        const bytes_written = std.unicode.utf8Encode(codepoint, buffer[index..]) catch return error.InvalidUnicodeScalar;

                        index += bytes_written;
                        i = end_hex + 1; // 跳过整个 \u{...}
                    },

                    // 未知转义：保留原样
                    else => {
                        buffer[index] = '\\';
                        buffer[index + 1] = c;
                        index += 2;
                        i += 2;
                    },
                }
            } else {
                // 普通字符，直接拷贝
                buffer[index] = input[i];
                index += 1;
                i += 1;
            }
        }

        // 调整切片大小为实际长度
        if (self.ast_arena.allocator().resize(buffer, index)) {
            return buffer[0..index];
        } else {
            const new_buf = try self.ast_arena.allocator().dupe(u8, buffer[0..index]);
            return new_buf;
        }
    }

    // ==========================================
    // Error Handling & Synchronization
    // ==========================================

    fn errorAtCurrent(self: *Parser, tag: ParseErrorTag) !void {
        return self.reportError(self.peek(), tag);
    }

    fn reportError(self: *Parser, token: Token, tag: ParseErrorTag) !void {
        if (self.panic_mode) return;
        self.panic_mode = true;

        const token_text = token.span.slice(self.source);
        const allocator = self.context.diag_arena.allocator();

        // 调用 Enum 自己的 format 方法
        const msg = try tag.message(allocator, token_text);

        // 传入生成的 msg
        try self.context.report(token.span, .Error, msg);
    }

    /// 错误恢复：跳过 token 直到语句边界
    pub fn synchronize(self: *Parser) void {
        self.panic_mode = false;

        while (self.peek().tag != .Eof) {
            if (self.stream.peek(0).tag == .Semicolon) { // 上一个是分号
                _ = self.advance();
                return;
            }

            // 如果遇到这几个关键词，大概率是新语句开始了
            switch (self.peek().tag) {
                .Fn, .Let, .Const, .Struct, .Enum, .If, .For, .Return => return,
                else => _ = self.advance(),
            }
        }
    }

    // ==========================================
    // Parse Entry Points
    // ==========================================

    /// 示例：解析 Identifier
    fn parseIdentifier(self: *Parser) !ast.Identifier {
        const token = try self.expect(.Identifier);
        const sym_id = try self.internToken(token);
        return ast.Identifier{
            .name = sym_id,
            .span = token.span,
        };
    }

    /// 优先级层级 (从低到高)
    const Precedence = enum(u8) {
        Lowest,
        Assignment, // = += -=
        Range, // ..
        LogicalOr, // or
        LogicalAnd, // and
        Equality, // == !=
        Comparison, // < > <= >=
        Bitwise, // | & ^
        Shift, // << >>
        Term, // + -
        Factor, // * / %
        Prefix, // - ! ~ & (前缀)
        Call, // . () [] .? .& .< (后缀)

        /// 将逻辑层级转换为实际的 Binding Power 数字
        /// 使用 inline fn 确保它在编译期就内联展开，没有任何运行时开销
        inline fn getBp(p: Precedence) u8 {
            // 乘以 10 是为了留出足够的空隙 (gap)，以防未来有什么奇怪的需求
            // 其实乘以 2 就够了，但 10 看着更宽敞，调试打印时也容易看
            return @intFromEnum(p) * 10;
        }

        fn getTokenPrecedence(tag: TokenType) Precedence {
            return switch (tag) {
                .PlusAssign, .MinusAssign, .StarAssign, .SlashAssign, .PercentAssign, .AmpersandAssign, .PipeAssign, .CaretAssign, .LShiftAssign, .RShiftAssign => .Assignment,

                .DotDot, .DotDotEqual => .Range,

                .Or, .Question => .LogicalOr,
                .And => .LogicalAnd,

                .Equal, .NotEqual => .Equality,

                .LessThan, .LessEqual, .GreaterThan, .GreaterEqual => .Comparison,

                .Pipe, .Caret, .Ampersand => .Bitwise,
                .LShift, .RShift => .Shift,

                .Plus, .Minus => .Term,
                .Star, .Slash, .Percent => .Factor,

                // 后缀操作符 (Suffix / Call)
                .Dot,
                .Bang,
                .LParen,
                .LBracket,
                .DotQuestion, // .?
                .DotStar, // .*
                .DotLessThan, // .<
                .LBrace,
                => .Call,

                else => .Lowest,
            };
        }
    };

    fn parseExpressionInternal(self: *Parser, min_precedence: Precedence, allow_struct_init: bool) ParseError!ast.Expression {
        var left = try self.parsePrefix();

        while (true) {
            const peek_token = self.peek();
            const next_prec = Precedence.getTokenPrecedence(peek_token.tag);

            if (@intFromEnum(next_prec) <= @intFromEnum(min_precedence)) {
                break;
            }

            // 结构体初始化限制检查
            // 如果下一个是 '{' 且当前禁止结构体初始化，则强制停止，将 '{' 留给外层语句（如 if/match）
            if (peek_token.tag == .LBrace and !allow_struct_init) {
                break;
            }

            _ = self.advance(); // eat op
            left = try self.parseInfix(left, peek_token.tag, next_prec);
        }

        return left;
    }

    // 标准模式：允许所有语法
    fn parseExpression(self: *Parser, min_precedence: Precedence) ParseError!ast.Expression {
        return self.parseExpressionInternal(min_precedence, true);
    }

    // 限制模式：遇到 '{' 立即停止
    // 用于 if condition, match target, for post-expr 等位置
    fn parseExpressionNoStruct(self: *Parser, min_precedence: Precedence) ParseError!ast.Expression {
        return self.parseExpressionInternal(min_precedence, false);
    }

    /// 解析中缀表达式
    /// lhs: 已经解析好的左边部分
    /// op: 运算符
    /// prec: 当前运算符的优先级
    fn parseInfix(self: *Parser, lhs: ast.Expression, op: TokenType, prec: Precedence) !ast.Expression {
        // === 第一类：后缀操作符 (Suffix / Call) ===
        // 这些操作符直接消费 lhs，并且拥有最高的结合力 (Call Precedence)
        // 它们通常不遵循标准的二元运算“左/右结合”规则，而是有自己独立的解析逻辑
        switch (op) {
            // 1. 错误传播/Try: val.?
            .DotQuestion => {
                const prop = try self.create(ast.PropagateExpression, .{
                    .operand = lhs,
                    .span = lhs.span().merge(self.stream.prev_token_span),
                });
                return .{ .Propagate = prop };
            },

            // 2. 解引用: val.*
            .DotStar => {
                const deref = try self.create(ast.UnaryExpression, .{
                    .operator = .Dereference,
                    .operand = lhs,
                    .span = lhs.span().merge(self.stream.prev_token_span),
                });
                return .{ .Unary = deref };
            },

            // 3. 泛型实例化/调用: expr.<T, U>
            .DotLessThan => {
                // 解析 <...> 参数列表
                const args = try self.parseGenericArguments();
                // 此时 current 已经在 > 后面，prev_token_span 是 >

                const gen_expr = try self.create(ast.GenericInstantiationExpression, .{
                    .base = lhs,
                    .arguments = args,
                    .span = lhs.span().merge(self.stream.prev_token_span),
                });
                return .{ .GenericInstantiation = gen_expr };
            },

            // 4. 函数调用: func(a, b)
            .LParen => {
                const args = try self.parseCallArguments();

                const call_expr = try self.create(ast.FunctionCallExpression, .{
                    .callee = lhs,
                    .arguments = args,
                    .span = lhs.span().merge(self.stream.prev_token_span),
                });
                return .{ .FunctionCall = call_expr };
            },

            // 5. 索引访问: arr[index]
            .LBracket => {
                const index = try self.parseExpression(.Lowest);
                const end_token = try self.expect(.RBracket);

                const index_expr = try self.create(ast.IndexAccessExpression, .{
                    .collection = lhs,
                    .index = index,
                    .span = lhs.span().merge(end_token.span),
                });
                return .{ .IndexAccess = index_expr };
            },

            // 6. 成员访问: obj.field
            .Dot => {
                const name_token = try self.expect(.Identifier);
                const sym = try self.internToken(name_token);

                const member_expr = try self.create(ast.MemberAccessExpression, .{
                    .object = lhs,
                    .member_name = sym,
                    .span = lhs.span().merge(name_token.span),
                });
                return .{ .MemberAccess = member_expr };
            },

            // 7. 宏调用后缀操作符 !
            // 语法: expr! ...
            // expr 可以是 identifier (vec), 也可以是 path (std.debug.print)
            .Bang => {
                // 1. 解析参数 Token Tree
                const args = try self.parseMacroArguments();
                const end_span = self.stream.prev_token_span;

                // 2. 构造 AST
                const macro_node = try self.create(ast.MacroCallExpression, .{
                    .callee = lhs, // 左边的表达式直接作为 callee
                    .arguments = args,
                    .span = lhs.span().merge(end_span),
                });

                return .{ .MacroCall = macro_node };
            },

            // 8. 范围语法 .. and ..=
            .DotDot, .DotDotEqual => {
                const is_inclusive = (op == .DotDotEqual);

                var rhs: ?ast.Expression = null;
                // 检查是否还有右值
                const next = self.peek();
                if (next.tag != .RBracket and next.tag != .RParen and next.tag != .Comma and next.tag != .Semicolon) {
                    rhs = try self.parseExpression(.Lowest);
                }

                if (is_inclusive and rhs == null) {
                    try self.errorAtCurrent(.UnexpectedToken); // "..=" must have an upper bound
                }

                const end_span = if (rhs) |r| r.span() else self.stream.prev_token_span;

                const range_node = try self.create(ast.RangeExpression, .{
                    .start = lhs,
                    .end = rhs,
                    .is_inclusive = is_inclusive, // 传入标志
                    .span = lhs.span().merge(end_span),
                });
                return .{ .Range = range_node };
            },

            // 9. 结构体初始化: Type { ... }
            // 场景: List.<i32> { x: 1 }
            // 此时:
            //   lhs = GenericInstantiation (List.<i32>)
            //   op  = .LBrace ({) -> 此处 '{' 已经被 advance() 吃掉了
            .LBrace => {
                const allocator = self.ast_arena.allocator();
                // 因为 '{' 已经被吃掉了，直接开始解析字段列表

                var fields = std.ArrayList(ast.StructFieldInit).empty;

                // 循环直到遇到 '}'
                while (!self.check(.RBrace) and !self.check(.Eof)) {
                    // 1. 解析字段名
                    const name_tok = try self.expect(.Identifier);
                    const name_sym = try self.internToken(name_tok);
                    var value_expr: ast.Expression = undefined;

                    // 2. 检查冒号 field: value
                    if (self.match(&.{.Colon})) {
                        value_expr = try self.parseExpression(.Lowest);
                    } else {
                        // 简写模式: field (等同于 field: field)
                        value_expr = .{ .Identifier = .{ .name = name_sym, .span = name_tok.span } };
                    }

                    try fields.append(allocator, .{
                        .name = name_sym,
                        .value = value_expr,
                        .span = name_tok.span.merge(value_expr.span()),
                    });

                    // 3. 处理逗号
                    if (!self.match(&.{.Comma})) break;
                }

                // 消耗闭合的 '}'
                const end_brace = try self.expect(.RBrace);

                // 创建 AST 节点
                const node = try self.create(ast.StructInitializationExpression, .{
                    .type_expression = lhs, // 左边的表达式 (如 List.<i32>) 就是类型
                    .fields = try fields.toOwnedSlice(allocator),
                    .span = lhs.span().merge(end_brace.span),
                });

                return .{ .StructInitialization = node };
            },

            // 如果不是后缀操作，那就进入下面的二元运算处理
            else => {},
        }

        // === 第二类：二元运算符 (Binary Operators) ===
        // + - * / = += == ...

        // 1. 处理右结合性
        // 比如赋值: a = b = c  =>  a = (b = c)
        const is_right_associative = switch (op) {
            .Assign, .PlusAssign, .MinusAssign, .StarAssign, .SlashAssign, .PercentAssign, .AmpersandAssign, .PipeAssign, .CaretAssign, .LShiftAssign, .RShiftAssign => true,
            else => false,
        };

        // 2. 计算右侧递归的 binding power
        // 左结合: next_prec = prec     (同级运算符不递归，立即停止，先算左边)
        // 右结合: next_prec = prec - 1 (同级运算符继续递归，先算右边)
        var next_min_prec = prec;
        if (is_right_associative) {
            const raw_val = @intFromEnum(prec);
            if (raw_val > 0) {
                next_min_prec = @enumFromInt(raw_val - 1);
            }
        }

        // 3. 递归解析右侧
        // 如果 op 是 Assign (=)，rhs 会解析出 b = c
        const rhs = try self.parseExpression(next_min_prec);

        // 4. 构造 AST
        if (is_right_associative and op == .Assign) {
            // 纯赋值 =
            const assign_expr = try self.create(ast.AssignmentExpression, .{
                .operator = .Assign,
                .target = lhs, // 左值检查通常放在语义分析阶段
                .value = rhs,
                .span = lhs.span().merge(rhs.span()),
            });
            return .{ .Assignment = assign_expr };
        } else if (is_right_associative) {
            // 复合赋值 +=, -= 等
            // 需要把 TokenType 转换为 AssignmentOperator
            const assign_op = ast.AssignmentOperator.fromToken(op);
            const assign_expr = try self.create(ast.AssignmentExpression, .{
                .operator = assign_op,
                .target = lhs,
                .value = rhs,
                .span = lhs.span().merge(rhs.span()),
            });
            return .{ .Assignment = assign_expr };
        } else {
            // 普通二元运算 + - * /
            const bin_expr = try self.create(ast.BinaryExpression, .{
                .operator = ast.BinaryOperator.fromToken(op),
                .left = lhs,
                .right = rhs,
                .span = lhs.span().merge(rhs.span()),
            });
            return .{ .Binary = bin_expr };
        }
    }

    // ==========================================
    // Pratt Parser: Prefix
    // ==========================================

    fn parsePrefix(self: *Parser) !ast.Expression {
        const allocator = self.ast_arena.allocator();
        const token = self.peek();

        switch (token.tag) {
            // === 1. 字面量 ===
            .IntLiteral, .FloatLiteral, .CharLiteral => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                const kind: ast.Literal.Kind = switch (tok.tag) {
                    .IntLiteral => .Integer,
                    .FloatLiteral => .Float,
                    .CharLiteral => .Character,
                    else => unreachable,
                };
                return .{ .Literal = .{ .kind = kind, .value = sym, .span = tok.span } };
            },

            .StringLiteral => {
                const tok = self.advance();
                const sym = try self.parseStringLiteral(tok);
                return .{ .Literal = .{ .kind = .String, .value = sym, .span = tok.span } };
            },

            .True, .False => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                return .{ .Literal = .{ .kind = .Boolean, .value = sym, .span = tok.span } };
            },

            .Undef => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                return .{ .Literal = .{ .kind = .Undef, .value = sym, .span = tok.span } };
            },

            .Unreach => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                return .{ .Literal = .{ .kind = .Unreachable, .value = sym, .span = tok.span } };
            },

            // === 2. 标识符 & 结构体初始化 ===
            .Identifier => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                const ident_expr: ast.Expression = .{ .Identifier = .{ .name = sym, .span = tok.span } };

                // 检查是否是结构体初始化: Ident { ... }
                // 如果后面紧跟 `{`，且 `{` 后面紧跟 `Ident :` 或 `Ident ,` 或 `Ident }` (简写) 或 `}` (空)，
                // 认为是结构体初始化。
                // 避免与 `if x { block }` 混淆
                if (self.check(.LBrace)) {
                    // 看得更远一点 (Peek 2 steps)
                    const next_next = self.stream.peek(1); // peek(0) is {, peek(1) is inside

                    const is_struct_init = switch (next_next.tag) {
                        .RBrace => true, // Empty: Ident {}
                        .Identifier => blk: {
                            // 检查 Ident 后面是什么: Ident : value OR Ident , OR Ident }
                            const third = self.stream.peek(2);
                            break :blk (third.tag == .Colon or third.tag == .Comma or third.tag == .RBrace);
                        },
                        else => false, // 比如 Ident { let ... } -> Block
                    };

                    if (is_struct_init) {
                        return self.parseStructInitialization(ident_expr);
                    }
                }

                return ident_expr;
            },

            // === 3. 括号：分组 (Expr) 或 元组 (A, B) ===
            .LParen => {
                const start_token = self.advance(); // eat '('

                // 3.1 空元组 () / Unit
                if (self.check(.RParen)) {
                    const end_token = self.advance();
                    // 分配一个空切片
                    const empty_elements = try self.allocList(ast.Expression, 0);
                    const tuple_expr = try self.create(ast.TupleInitializationExpression, .{
                        .elements = empty_elements,
                        .span = start_token.span.merge(end_token.span),
                    });
                    return .{ .TupleInitialization = tuple_expr };
                }

                // 3.2 解析第一个表达式
                const first_expr = try self.parseExpression(.Lowest);

                // 3.3 检查是否是元组 (看有没有逗号)
                if (self.match(&.{.Comma})) {
                    var elements = std.ArrayList(ast.Expression).empty;
                    try elements.append(allocator, first_expr);

                    // 循环解析剩下的元素
                    while (!self.check(.RParen) and !self.check(.Eof)) {
                        try elements.append(allocator, try self.parseExpression(.Lowest));
                        if (!self.match(&.{.Comma})) break;
                    }

                    const end_token = try self.expect(.RParen);

                    const tuple_expr = try self.create(ast.TupleInitializationExpression, .{
                        .elements = try elements.toOwnedSlice(allocator),
                        .span = start_token.span.merge(end_token.span),
                    });
                    return .{ .TupleInitialization = tuple_expr };
                } else {
                    // 没有逗号，只是普通分组 (Expr)
                    _ = try self.expect(.RParen);
                    // 这里不需要创建 Grouping 节点，直接返回内部表达式即可
                    // 但需要注意 Span，如果需要在 AST 中保留括号信息，可以加 GroupingNode
                    // 为了简化，我们直接返回内部 Expr，但这样 Span 信息会丢失括号范围
                    // 如果 Loom 需要精准报错，通常不加 Grouping 节点也够用
                    return first_expr;
                }
            },

            // === 4. 一元运算 / 引用 ===
            .Minus, .Bang, .Tilde, .Hash, .Question => {
                const op_token = self.advance();
                const right = try self.parseExpression(.Prefix);

                const op: ast.UnaryOperator = switch (op_token.tag) {
                    .Minus => .Negate,
                    .Bang => .LogicalNot,
                    .Tilde => .BitwiseNot,
                    .Hash => .LengthOf,
                    .Question => .Optional,
                    else => unreachable,
                };

                const unary = try self.create(ast.UnaryExpression, .{
                    .operator = op,
                    .operand = right,
                    .span = op_token.span.merge(right.span()),
                });
                return .{ .Unary = unary };
            },

            // &x (取地址) 或 &T (指针类型)
            .Ampersand => {
                const start_token = self.advance();

                // 检查是否是 &mut T (指针类型)

                // 1. 如果后面是 mut 关键字
                if (self.match(&.{.Mut})) {
                    const sub_type = try self.parseExpression(.Prefix);
                    const ptr_type = try self.create(ast.PointerTypeExpression, .{
                        .is_mutable = true,
                        .is_volatile = false,
                        .child_type = sub_type,
                        .span = start_token.span.merge(sub_type.span()),
                    });
                    return .{ .PointerType = ptr_type };
                }

                // 2. 普通 &Expr
                // 统一解析为 Unary AddressOf。

                const right = try self.parseExpression(.Prefix);
                const unary = try self.create(ast.UnaryExpression, .{
                    .operator = .AddressOf,
                    .operand = right,
                    .span = start_token.span.merge(right.span()),
                });
                return .{ .Unary = unary };
            },

            // *T (Volatile 指针类型 或 C 指针)
            // Loom 规范提到 *mut 用于驱动开发
            .Star => {
                const start_token = self.advance();
                const is_mut = self.match(&.{.Mut}); // 匹配 mut
                const sub_type = try self.parseExpression(.Prefix);

                const ptr_type = try self.create(ast.PointerTypeExpression, .{
                    .is_mutable = is_mut,
                    .is_volatile = true, // * 号表示 volatile
                    .child_type = sub_type,
                    .span = start_token.span.merge(sub_type.span()),
                });
                return .{ .PointerType = ptr_type };
            },

            // === 5. 数组/切片: [1, 2], [N]T, []T ===
            .LBracket => {
                const start_token = self.advance();

                // 5.1 Slice Type: []T
                if (self.check(.RBracket)) {
                    _ = self.advance(); // eat ']'
                    // [] 后面紧跟的一定是类型
                    const child_type = try self.parseExpression(.Prefix);

                    const slice_type = try self.create(ast.SliceTypeExpression, .{
                        .child_type = child_type,
                        .span = start_token.span.merge(child_type.span()),
                    });
                    return .{ .SliceType = slice_type };
                }

                // 5.2 解析第一个表达式
                const first_expr = try self.parseExpression(.Lowest);

                // Case A: [0; 10] (Repeat Init)
                if (self.match(&.{.Semicolon})) {
                    const count_expr = try self.parseExpression(.Lowest);
                    const end_token = try self.expect(.RBracket);

                    const array_init = try self.create(ast.ArrayInitializationExpression, .{
                        .elements = try self.singleElementSlice(first_expr),
                        .repeat_count = count_expr,
                        .span = start_token.span.merge(end_token.span),
                    });
                    return .{ .ArrayInitialization = array_init };
                }

                // Case B: [1, 2] or [1,] (List Init)
                // 只要看到逗号，就百分之百是字面量，不再可能是类型
                else if (self.match(&.{.Comma})) {
                    var elements = std.ArrayList(ast.Expression).empty;
                    try elements.append(allocator, first_expr);

                    // 允许尾随逗号 [1,]
                    while (!self.check(.RBracket) and !self.check(.Eof)) {
                        try elements.append(allocator, try self.parseExpression(.Lowest));
                        if (!self.match(&.{.Comma})) break;
                    }
                    const end_token = try self.expect(.RBracket);

                    const array_init = try self.create(ast.ArrayInitializationExpression, .{
                        .elements = try elements.toOwnedSlice(allocator),
                        .repeat_count = null,
                        .span = start_token.span.merge(end_token.span),
                    });
                    return .{ .ArrayInitialization = array_init };
                }

                // Case C: [Expr] -> 歧义区
                // 可能是 [10] (单元素数组) 或 [10]u8 (数组类型)
                else {
                    const end_token = try self.expect(.RBracket); // eat ']'

                    // 策略：贪婪匹配类型
                    // 如果下一个符号看起来像类型开头，我们优先尝试解析为数组类型。
                    // 这意味着 [10] * 2 会被误判为类型解析，导致后续报错。
                    // 必须写 [10,] * 2
                    if (isTypeStart(self.peek())) {
                        const child_type = try self.parseExpression(.Prefix);
                        const array_type = try self.create(ast.ArrayTypeExpression, .{
                            .size = first_expr,
                            .child_type = child_type,
                            .span = start_token.span.merge(child_type.span()),
                        });
                        return .{ .ArrayType = array_type };
                    } else {
                        // 下一个符号不像类型 (比如 +, -, /, EOF, ;)
                        // 安全地解析为单元素数组字面量
                        const array_init = try self.create(ast.ArrayInitializationExpression, .{
                            .elements = try self.singleElementSlice(first_expr),
                            .repeat_count = null,
                            .span = start_token.span.merge(end_token.span),
                        });
                        return .{ .ArrayInitialization = array_init };
                    }
                }
            },

            // === 6. 控制流表达式 ===
            .If => return self.parseIfExpression(),
            .Match => return self.parseMatchExpression(),
            .LBrace => return self.parseBlockExpression(), // { ... }

            // === 7. 类型声明表达式 (fn, struct) ===
            .Fn => {
                return self.parseFunctionType();
            },

            // === 8. 范围语法 (range) ===
            .DotDot, .DotDotEqual => {
                const start_tok = self.advance();
                const is_inclusive = (start_tok.tag == .DotDotEqual);

                // 解析右侧
                const rhs = try self.parseExpression(.Prefix);

                const range_node = try self.create(ast.RangeExpression, .{
                    .start = null,
                    .end = rhs,
                    .is_inclusive = is_inclusive,
                    .span = start_tok.span.merge(rhs.span()),
                });
                return .{ .Range = range_node };
            },

            else => return error.ParseError,
        }
    }

    // 解析 <T, U>
    // 注意：进入此函数时，DotLessThan (.<) 已经被 parseInfix 消费了，
    // 但是如果是 TypeContext 下的 <T>，则是 < 被消费。
    // 这里我们假设这是通用的参数列表解析逻辑。
    fn parseGenericArguments(self: *Parser) ![]ast.Expression {
        const allocator = self.ast_arena.allocator();
        var args = std.ArrayList(ast.Expression).empty;

        if (!self.check(.GreaterThan)) {
            while (true) {
                const arg = try self.parseType();
                try args.append(allocator, arg);
                if (!self.match(&.{.Comma})) break;
            }
        }

        _ = try self.expect(.GreaterThan);
        return args.toOwnedSlice(allocator);
    }

    // 解析 (a: 1, b, c)
    fn parseCallArguments(self: *Parser) ![]ast.CallArgument {
        const allocator = self.ast_arena.allocator();
        var args = std.ArrayList(ast.CallArgument).empty;

        if (!self.check(.RParen)) {
            while (true) {
                var name: ?SymbolId = null;
                var start_span = self.peek().span; // 记录参数起始位置

                // 检查是否是命名参数: Identifier + Colon
                // 我们需要看一下 peek(1)
                // 注意：self.stream.peek(1) 可能会越界吗？TokenStream 实现通常会返回 Eof，所以是安全的。
                if (self.check(.Identifier) and self.stream.peek(1).tag == .Colon) {
                    const name_tok = self.advance(); // eat identifier
                    name = try self.internToken(name_tok);
                    _ = self.advance(); // eat colon
                }

                // 解析值部分
                const value = try self.parseExpression(.Lowest);

                // 合并 span：如果有 name，从 name 开始；否则从 value 开始
                const arg_span = if (name) |_| start_span.merge(value.span()) else value.span();

                try args.append(allocator, .{
                    .name = name,
                    .value = value,
                    .span = arg_span,
                });

                if (!self.match(&.{.Comma})) break;
            }
        }

        _ = try self.expect(.RParen);
        return args.toOwnedSlice(allocator);
    }

    /// 解析宏调用的参数 (Token Tree)
    /// 支持 (), [], {}
    fn parseMacroArguments(self: *Parser) ![]Token {
        const allocator = self.ast_arena.allocator();
        // 1. 确定定界符
        const start_token = self.peek();
        var end_type: TokenType = undefined;

        if (self.match(&.{.LParen})) {
            end_type = .RParen;
        } else if (self.match(&.{.LBracket})) {
            end_type = .RBracket;
        } else if (self.match(&.{.LBrace})) {
            end_type = .RBrace;
        } else {
            // 宏调用必须紧跟定界符: vec! [ ... ]
            try self.errorAtCurrent(.UnexpectedToken);
            return error.ParseError;
        }

        // 2. 收集 Token 直到括号平衡
        var tokens = std.ArrayList(Token).empty;
        var nesting: usize = 1;

        while (nesting > 0 and !self.check(.Eof)) {
            const t = self.advance();

            // 检查嵌套
            if (t.tag == start_token.tag) {
                nesting += 1;
            } else if (t.tag == end_type) {
                nesting -= 1;
                if (nesting == 0) break; // 结束
            }

            try tokens.append(allocator, t);
        }

        // 如果循环结束 nesting 还不为 0，说明文件结束了但括号没闭合
        if (nesting > 0) {
            try self.errorAtCurrent(.UnexpectedToken);
            return error.ParseError;
        }

        return tokens.toOwnedSlice(allocator);
    }

    // ==========================================
    // Pattern Parsing (用于 let, match, fn args)
    // ==========================================

    fn parsePattern(self: *Parser) ParseError!ast.Pattern {
        const allocator = self.ast_arena.allocator();
        const token = self.peek();

        switch (token.tag) {
            // 1. 通配符 _
            .Underscore => {
                const tok = self.advance();
                return .{ .Wildcard = tok.span };
            },
            // 2. 标识符
            .Identifier => return self.parseIdentifierPattern(),
            // 3. 字面量匹配 (1, "abc", true)
            .IntLiteral, .FloatLiteral, .StringLiteral, .CharLiteral, .True, .False, .Null => {
                // 1. 解析左值
                const start_expr = try self.parseExpression(.Prefix);
                if (start_expr != .Literal) return error.ParseError;
                const start_lit = start_expr.Literal;

                // 2. 检查是否有范围操作符 (.. 或 ..=)
                if (self.check(.DotDot) or self.check(.DotDotEqual)) {
                    const op_token = self.advance(); // 吃掉操作符
                    const is_inclusive = (op_token.tag == .DotDotEqual);

                    // 3. 解析右值
                    const end_expr = try self.parseExpression(.Prefix);
                    if (end_expr != .Literal) return error.ParseError;
                    const end_lit = end_expr.Literal;

                    return .{
                        .Range = .{
                            .start = start_lit,
                            .end = end_lit,
                            .is_inclusive = is_inclusive, // 传入标志
                            .span = start_lit.span.merge(end_lit.span),
                        },
                    };
                }

                return .{ .Literal = start_lit };
            },

            // 4. 元组解构 (a, b)
            .LParen => {
                const start = self.advance();
                var elements = std.ArrayList(ast.Pattern).empty;

                while (!self.check(.RParen) and !self.check(.Eof)) {
                    try elements.append(allocator, try self.parsePattern());
                    if (!self.match(&.{.Comma})) break;
                }
                const end = try self.expect(.RParen);

                return .{ .TupleDestructuring = .{
                    .elements = try elements.toOwnedSlice(allocator),
                    .span = start.span.merge(end.span),
                } };
            },

            // 5. Enum 简写匹配 (.Ok(v))
            .Dot => {
                const start = self.advance(); // eat .
                const name = try self.expect(.Identifier);
                const sym = try self.internToken(name);

                var payloads = std.ArrayList(ast.Pattern).empty;
                if (self.match(&.{.LParen})) {
                    while (!self.check(.RParen)) {
                        try payloads.append(allocator, try self.parsePattern());
                        if (!self.match(&.{.Comma})) break;
                    }
                    _ = try self.expect(.RParen);
                }

                // 结束位置：如果有 payload 则是 payload 的结束，否则是 name
                // 这里简化处理
                return .{
                    .EnumMatching = .{
                        .variant_name = sym,
                        .type_context = null, // .Ok 意味着推导类型
                        .payloads = try payloads.toOwnedSlice(allocator),
                        .span = start.span.merge(name.span), // 粗略 span
                    },
                };
            },

            // 6. mut x (可变绑定)
            .Mut => {
                const start = self.advance();
                const name_tok = try self.expect(.Identifier);
                const sym = try self.internToken(name_tok);
                return .{ .IdentifierBinding = .{
                    .name = sym,
                    .is_mutable = true,
                    .span = start.span.merge(name_tok.span),
                } };
            },

            else => return error.ParseError,
        }
    }

    fn parseIdentifierPattern(self: *Parser) !ast.Pattern {
        const allocator = self.ast_arena.allocator();
        // ==========================================
        // 1. 构建类型路径 / 基础表达式
        //    (支持 Identifier 和 GenericInstantiation)
        // ==========================================

        // 1.1 读取起始标识符
        const start_tok = try self.expect(.Identifier);
        const start_sym = try self.internToken(start_tok);

        // 初始化当前构建的表达式 (base)
        // 这可能是变量名，也可能是类型名 (Point)，也可能是 Enum 名 (Result)
        // 我们先把它构建在栈上，如果需要升级再移到堆上
        var current_expr: ast.Expression = .{ .Identifier = .{ .name = start_sym, .span = start_tok.span } };

        // 1.2 检查泛型参数 <T>
        // 例如: Result<i32> 或 Struct<T>
        if (self.check(.LessThan)) {
            _ = self.advance(); // 吃掉 <
            const args = try self.parseGenericArguments(); // 吃掉 ...>
            const end_span = self.stream.prev_token_span; // > 的位置

            // 将 Identifier 升级为 GenericInstantiation
            const base_ptr = try self.create(ast.Expression, current_expr);

            const gen_node = try self.create(ast.GenericInstantiationExpression, .{
                .base = base_ptr.*,
                .arguments = args,
                .span = start_tok.span.merge(end_span),
            });

            // 更新 current_expr
            current_expr = .{ .GenericInstantiation = gen_node };
        }

        // ==========================================
        // 2. 分支决策：Enum? Struct? Binding?
        // ==========================================

        // Case A: 全路径 Enum 匹配 (Type.Variant)
        // 例如: Result<i32>.Ok(v) 或 Color.Red
        if (self.match(&.{.Dot})) {
            // 2.1 解析变体名
            const variant_tok = try self.expect(.Identifier);
            const variant_sym = try self.internToken(variant_tok);

            // 2.2 解析 Payload (可选)
            // .Ok(v)
            var payloads = std.ArrayList(ast.Pattern).empty;
            if (self.match(&.{.LParen})) {
                while (!self.check(.RParen)) {
                    try payloads.append(allocator, try self.parsePattern());
                    if (!self.match(&.{.Comma})) break;
                }
                _ = try self.expect(.RParen);
            }

            const end_span = self.stream.prev_token_span;

            // 构造 EnumMatchingPattern
            // 注意：type_context 就是刚才解析出来的 current_expr (Result<i32>)
            return .{ .EnumMatching = .{
                .variant_name = variant_sym,
                .type_context = current_expr,
                .payloads = try payloads.toOwnedSlice(allocator),
                .span = start_tok.span.merge(end_span),
            } };
        }

        // Case B: 结构体解构 (Type { ... })
        // 例如: Point { x, y } 或 Point<i32> { x, .. }
        if (self.check(.LBrace)) {
            _ = self.advance(); // eat '{'

            var fields = std.ArrayList(ast.PatternStructField).empty;
            var ignore_remaining = false; // 是否包含 ..

            while (!self.check(.RBrace) and !self.check(.Eof)) {
                // 处理剩余模式 ..
                if (self.match(&.{.DotDot})) {
                    ignore_remaining = true;
                    // .. 必须是最后一个字段
                    if (!self.check(.RBrace)) {
                        try self.errorAtCurrent(.UnexpectedToken);
                    }
                    // 吃掉可能存在的逗号，直接结束循环
                    _ = self.match(&.{.Comma});
                    break;
                }

                // 检查 mut 修饰符 (用于简写形式)
                var is_field_mut = false;
                var field_start_span = self.peek().span;
                if (self.match(&.{.Mut})) {
                    is_field_mut = true;
                }

                // 普通字段解析
                const field_name_tok = try self.expect(.Identifier);
                const field_sym = try self.internToken(field_name_tok);
                var sub_pat: ast.Pattern = undefined;

                if (self.match(&.{.Colon})) {
                    // 完整写法: x: pattern
                    // 如果前面写了 mut (如: mut x: y)，这是非法的语法。
                    // mut 应该属于 pattern 的一部分 (如: x: mut y)。
                    if (is_field_mut) {
                        try self.errorAtCurrent(.UnexpectedToken); // mut 不能放在 key 前面
                    }
                    // 完整写法: x: pattern
                    sub_pat = try self.parsePattern();
                } else {
                    // 简写: x  =>  x: x (Binding)
                    // 或: mut x => x: mut x
                    sub_pat = .{ .IdentifierBinding = .{
                        .name = field_sym,
                        .is_mutable = is_field_mut,
                        .span = if (is_field_mut) field_start_span.merge(field_name_tok.span) else field_name_tok.span,
                    } };
                }

                try fields.append(allocator, .{
                    .field_name = field_sym,
                    .pattern = sub_pat,
                    .span = field_name_tok.span.merge(sub_pat.span()),
                });

                if (!self.match(&.{.Comma})) break;
            }

            const rbrace = try self.expect(.RBrace);

            return .{
                .StructDestructuring = .{
                    .type_expression = current_expr,
                    .fields = try fields.toOwnedSlice(allocator),
                    .ignore_remaining = ignore_remaining, // [AST 已更新]
                    .span = start_tok.span.merge(rbrace.span),
                },
            };
        }

        if (current_expr == .GenericInstantiation) {
            // 报错：变量绑定不能带泛型参数
            try self.errorAtCurrent(.UnexpectedToken);
            return error.ParseError;
        }

        return .{
            .IdentifierBinding = .{
                .name = start_sym,
                .is_mutable = false, // 如果是 mut x，是在 parsePattern 入口处理的 Var 分支，不是这里
                .span = start_tok.span,
            },
        };
    }

    /// 解析函数类型表达式
    /// 语法: fn(ParamType, ...) ReturnType
    /// 示例: fn(i32, f32) bool
    fn parseFunctionType(self: *Parser) !ast.Expression {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'fn'

        _ = try self.expect(.LParen);

        var params = std.ArrayList(ast.Expression).empty;
        var is_variadic = false;

        // 1. 解析参数列表
        if (!self.check(.RParen)) {
            while (true) {
                // 处理 C FFI 变长参数 (...)
                if (self.match(&.{.Ellipsis})) {
                    is_variadic = true;
                    break; // 变长参数必须位于末尾
                }

                // 解析参数类型
                // fn(i32, u8)
                const param_type = try self.parseType();
                try params.append(allocator, param_type);

                if (!self.match(&.{.Comma})) break;
            }
        }

        _ = try self.expect(.RParen);

        // 2. 解析返回值类型
        var return_type: ?ast.Expression = null;

        // 启发式判断：如果下一个 Token 是类型的开头，则解析返回值。
        if (isTypeStart(self.peek())) {
            return_type = try self.parseType();
        }

        const end_span = if (return_type) |r| r.span() else self.stream.prev_token_span;

        const node = try self.create(ast.FunctionTypeExpression, .{
            .parameters = try params.toOwnedSlice(allocator),
            .return_type = return_type,
            .is_variadic = is_variadic,
            .span = start.span.merge(end_span),
        });

        return .{ .FunctionType = node };
    }

    // ==========================================
    // Type Parsing (专用于类型上下文)
    // ==========================================

    /// 解析类型
    /// 这里的规则与 parseExpression 不同：
    /// 1. `<` 直接被解析为泛型参数 (List<i32>)，不需要 .<
    /// 2. 支持范围类型语法: StartType .. EndType
    fn parseType(self: *Parser) ParseError!ast.Expression {
        // 1. 解析前缀 (Prefix)
        // 类型的起点通常是：Identifier, [, &, ?, fn, *, extern, (, !
        var left = try self.parseTypePrefix();

        // 2. 解析后缀 (Suffix) 和 中缀 (Infix for Type)
        // 类型通常只支持：
        // - .Member (命名空间引用 std.collections.List)
        // - <T> (泛型实例化)
        // - .. / ..= (范围类型) [NEW]

        while (true) {
            const token = self.peek();

            // === 泛型: List<T> ===
            if (token.tag == .LessThan) {
                // 消耗 < ... >
                _ = self.advance(); // <
                const args = try self.parseGenericArguments(); // ... >

                const end_span = self.stream.prev_token_span; // > 的位置

                // 构造 GenericInstantiationExpression
                const node = try self.create(ast.GenericInstantiationExpression, .{
                    .base = left,
                    .arguments = args,
                    .span = left.span().merge(end_span),
                });
                left = .{ .GenericInstantiation = node };
                continue;
            }

            // === 成员访问: std.List ===
            if (token.tag == .Dot) {
                _ = self.advance(); // eat .
                const name_tok = try self.expect(.Identifier);
                const sym = try self.internToken(name_tok);

                const node = try self.create(ast.MemberAccessExpression, .{
                    .object = left,
                    .member_name = sym,
                    .span = left.span().merge(name_tok.span),
                });
                left = .{ .MemberAccess = node };
                continue;
            }

            // === [NEW] 范围类型: T .. T ===
            // 例如: usize .. usize 或 u8 ..= u8
            // 逻辑：left 已经是完整解析好的左侧类型（包含泛型等），
            // 如果遇到 ..，则消耗它并递归解析右侧类型。
            if (token.tag == .DotDot or token.tag == .DotDotEqual) {
                const op_token = self.advance(); // eat .. or ..=
                const is_inclusive = (op_token.tag == .DotDotEqual);

                // 递归解析右侧类型
                // 注意：这里调用 parseType 是因为右侧也可以是复杂的类型 (如 std.List<i32>)
                const right = try self.parseType();

                const node = try self.create(ast.RangeExpression, .{
                    .start = left, // 左侧类型
                    .end = right, // 右侧类型
                    .is_inclusive = is_inclusive,
                    .span = left.span().merge(right.span()),
                });

                // Range 类型构造完成后，通常就不能再接 .Dot 或 <T> 了
                // (i32..i32).foo 这种语法在类型定义中极其罕见且有歧义
                // 所以我们直接返回结果
                return .{ .Range = node };
            }

            // 如果遇到其他符号 (比如 { = ; , ) )，说明类型解析结束
            break;
        }

        return left;
    }

    /// 解析类型的前缀部分
    fn parseTypePrefix(self: *Parser) !ast.Expression {
        const token = self.peek();

        switch (token.tag) {
            // 1. 标识符 (List, i32)
            .Identifier => {
                const tok = self.advance();
                const sym = try self.internToken(tok);
                return .{ .Identifier = .{ .name = sym, .span = tok.span } };
            },

            // 2. 指针 (&T, *T)
            .Ampersand, .Star => {
                const start = self.advance();
                // 检查 mut
                const is_mut = self.match(&.{.Mut});
                const is_volatile = (start.tag == .Star);

                // 递归解析子类型
                const child = try self.parseType();

                const node = try self.create(ast.PointerTypeExpression, .{
                    .is_mutable = is_mut,
                    .is_volatile = is_volatile,
                    .child_type = child,
                    .span = start.span.merge(child.span()),
                });
                return .{ .PointerType = node };
            },

            // 3. 数组/切片 ([]T, [N]T)
            .LBracket => {
                const start = self.advance();

                // 3.1 切片 []T
                if (self.match(&.{.RBracket})) {
                    const child = try self.parseType();
                    const node = try self.create(ast.SliceTypeExpression, .{
                        .child_type = child,
                        .span = start.span.merge(child.span()),
                    });
                    return .{ .SliceType = node };
                }

                // 3.2 数组 [N]T
                // 注意：N 是一个值表达式,这里要切回 parseExpression
                const size_expr = try self.parseExpression(.Lowest);
                _ = try self.expect(.RBracket);

                const child = try self.parseType();

                const node = try self.create(ast.ArrayTypeExpression, .{
                    .size = size_expr,
                    .child_type = child,
                    .span = start.span.merge(child.span()),
                });
                return .{ .ArrayType = node };
            },

            // 4. 可选类型 (?T)
            .Question => {
                const start = self.advance();
                const child = try self.parseType();
                const node = try self.create(ast.OptionalTypeExpression, .{
                    .child_type = child,
                    .span = start.span.merge(child.span()),
                });
                return .{ .OptionalType = node };
            },

            // 5. 函数类型 (fn(A) B)
            .Fn => {
                return self.parseFunctionType();
            },

            // 6. 不可达类型 (!)
            .Bang => {
                const tok = self.advance(); // eat '!'
                return .{ .NeverType = tok.span };
            },

            // 7. 元组类型
            .LParen => {
                return self.parsePrefix();
            },

            else => {
                try self.errorAtCurrent(.UnexpectedToken);
                return error.ParseError;
            },
        }
    }

    // ==========================================
    // Statement Parsing
    // ==========================================

    fn parseStatement(self: *Parser) ParseError!ast.Statement {
        const token = self.peek();

        switch (token.tag) {
            // 1. 本地绑定
            .Let => return self.parseLetStatement(),

            // 2. 控制流语句
            .For => return self.parseForStatement(),
            .Return => return self.parseReturnStatement(),
            .Defer => return self.parseDeferStatement(),
            .Break => return self.parseBreakStatement(),
            .Continue => return self.parseContinueStatement(),

            // 3. 嵌套声明
            // 允许在 Block 内部定义 fn, struct, enum, static, const 等
            .Fn, .Struct, .Enum, .Union, .Trait, .Impl, .Type, .Const, .Static, .Extern, .Use, .Macro => {
                const decl = try self.parseDeclaration();
                return .{ .Declaration = decl };
            },

            // 4. 表达式语句
            else => {
                const expr = try self.parseExpression(.Lowest);

                // Check 1: 显式分号 (Standard Case)
                // 如果有分号，那绝对是合法的语句
                if (self.match(&.{.Semicolon})) {
                    return .{ .ExpressionStatement = expr };
                }

                // Check 2: 块状表达式 (Block-like Expressions)
                // If, Match, Block 这三种表达式如果作为语句出现，允许省略分号
                // 例如:
                //    if true { ... }  <-- 没分号，合法
                //    print("hi");     <-- 必须有分号
                switch (expr) {
                    .If, .Match, .Block => {
                        return .{ .ExpressionStatement = expr };
                    },
                    else => {
                        // Check 3: 既没分号，也不是块状表达式 -> 报错
                        // 因为 parseStatement 不负责处理 "返回值"，所以这里必须报错
                        try self.errorAtCurrent(.ExpectedSemicolon);
                        return error.ParseError;
                    },
                }
            },
        }
    }

    // let pattern = value;
    fn parseLetStatement(self: *Parser) !ast.Statement {
        const start = self.advance(); // eat 'let'

        // 1. 解析模式
        // 如果写 "let mut x"，这里会调用 parsePattern -> 命中 .Mut 分支
        // 返回一个 is_mutable=true 的 Binding
        const pat = try self.parsePattern();

        // 2. 类型注解 (可选)
        var type_anno: ?ast.Expression = null;
        if (self.match(&.{.Colon})) {
            type_anno = try self.parseType();
        }

        // 3. 初始化值
        _ = try self.expect(.Assign);
        const value = try self.parseExpression(.Lowest);

        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.LetStatement, .{
            .pattern = pat,
            .type_annotation = type_anno,
            .value = value,
            .span = start.span.merge(end.span),
        });
        return .{ .Let = node };
    }

    fn parseReturnStatement(self: *Parser) !ast.Statement {
        const start = self.advance(); // eat return
        var value: ?ast.Expression = null;

        if (!self.check(.Semicolon)) {
            value = try self.parseExpression(.Lowest);
        }
        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.ReturnStatement, .{
            .value = value,
            .span = start.span.merge(end.span),
        });
        return .{ .Return = node };
    }

    fn parseDeferStatement(self: *Parser) !ast.Statement {
        const start = self.advance(); // eat 'defer'

        // 1. 解析要延迟执行的表达式
        // 这可以是函数调用 defer f.close()
        // 也可以是代码块 defer { ... }
        const target = try self.parseExpression(.Lowest);

        // 2. 强制要求分号
        // 既然规范是 defer expr; 那么无论 target 是什么，后面都必须跟分号。
        // 即便是 defer { ... }; 也要加分号。
        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.DeferStatement, .{
            .target = target,
            .span = start.span.merge(end.span),
        });
        return .{ .Defer = node };
    }

    fn parseBreakStatement(self: *Parser) !ast.Statement {
        const tok = self.advance(); // eat break
        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.BreakStatement, .{ .span = tok.span.merge(end.span) });
        return .{ .Break = node };
    }

    fn parseContinueStatement(self: *Parser) !ast.Statement {
        const tok = self.advance(); // eat continue
        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.ContinueStatement, .{ .span = tok.span.merge(end.span) });
        return .{ .Continue = node };
    }

    // ==========================================
    // Control Flow Expressions
    // ==========================================

    fn parseBlockExpression(self: *Parser) ParseError!ast.Expression {
        const allocator = self.ast_arena.allocator();
        const start = try self.expect(.LBrace);
        var stmts = std.ArrayList(ast.Statement).empty;
        var result_expr: ?ast.Expression = null;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 判定是否为语句/声明的开始
            if (
            // 1. 标准语句关键字
            self.check(.Let) or self.check(.For) or
                self.check(.Return) or self.check(.Defer) or
                self.check(.Break) or self.check(.Continue) or

                // 2. 声明关键字 (Declarations)
                // 这些关键字出现在块内时，也视为语句的一部分
                self.check(.Fn) or self.check(.Struct) or self.check(.Enum) or
                self.check(.Union) or self.check(.Trait) or self.check(.Impl) or
                self.check(.Type) or self.check(.Const) or self.check(.Static) or
                self.check(.Extern) or self.check(.Use) or self.check(.Macro))
            {
                try stmts.append(allocator, try self.parseStatement());
            } else {
                // 看起来是表达式
                const expr = try self.parseExpression(.Lowest);

                if (self.match(&.{.Semicolon})) {
                    // 1. 有分号 -> 是语句
                    try stmts.append(allocator, .{ .ExpressionStatement = expr });
                } else if (self.check(.RBrace)) {
                    // 2. 没分号，且紧跟着 } -> 是返回值
                    result_expr = expr;
                    break;
                } else {
                    // 3. 没分号，但它是 If/Match/Block -> 视为语句
                    switch (expr) {
                        .If, .Match, .Block => {
                            try stmts.append(allocator, .{ .ExpressionStatement = expr });
                        },
                        else => {
                            // 4. 其他情况 -> 报错
                            _ = try self.expect(.Semicolon);
                        },
                    }
                }
            }
        }
        const end = try self.expect(.RBrace);

        const block = try self.create(ast.BlockExpression, .{
            .statements = try stmts.toOwnedSlice(allocator),
            .result_expression = result_expr,
            .span = start.span.merge(end.span),
        });
        return .{ .Block = block };
    }

    fn parseIfExpression(self: *Parser) !ast.Expression {
        const start = self.advance(); // eat if
        const condition = try self.parseExpressionNoStruct(.Lowest);

        // 解析 then block (必须是 Block)
        // 这里的 parseBlockExpression 返回的是 Expression(Block)，需要转回 *BlockExpression
        const then_expr = try self.parseBlockExpression();
        const then_block = then_expr.Block; // 获取指针

        var else_branch: ?ast.Expression = null;
        if (self.match(&.{.Else})) {
            if (self.check(.If)) {
                // else if ... 递归解析
                else_branch = try self.parseIfExpression();
            } else {
                // else { ... }
                else_branch = try self.parseBlockExpression();
            }
        }

        const end_span = if (else_branch) |e| e.span() else then_block.span;

        const if_node = try self.create(ast.IfExpression, .{
            .condition = condition,
            .then_branch = then_block,
            .else_branch = else_branch,
            .span = start.span.merge(end_span),
        });
        return .{ .If = if_node };
    }

    fn parseMatchExpression(self: *Parser) !ast.Expression {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat match

        // 1. Target Expression (没有括号)
        const target = try self.parseExpressionNoStruct(.Lowest);

        _ = try self.expect(.LBrace);
        var arms = std.ArrayList(ast.MatchArm).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 2. Pattern
            const pattern = try self.parsePattern();

            // 3. Arrow =>
            _ = try self.expect(.Arrow);

            // 4. Body Expression
            // 可以是 Block，也可以是单行表达式
            const body = try self.parseExpression(.Lowest);

            // 5. 逗号 (Loom 规范里 match arm 之间通常要有逗号，除非是 Block)
            // 允许省略最后一个逗号
            _ = self.match(&.{.Comma});

            try arms.append(allocator, .{
                .pattern = pattern,
                .body = body,
                .span = pattern.span().merge(body.span()),
            });
        }

        const end = try self.expect(.RBrace);

        const match_node = try self.create(ast.MatchExpression, .{
            .target = target,
            .arms = try arms.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Match = match_node };
    }

    fn parseForStatement(self: *Parser) !ast.Statement {
        const start = self.advance(); // eat 'for'

        // === 1. 初始化部分 (Initializer) ===
        var initializer: ?*ast.Statement = null;

        if (self.check(.Let)) {
            // Case A: let mut i = 0;
            const stmt = try self.parseLetStatement();

            // 包装到堆上
            initializer = try self.create(ast.Statement, stmt);
        } else if (self.match(&.{.Semicolon})) {
            // Case B: 空初始化 (; i < 10; ...)
            initializer = null;
        } else {
            // Case C: 表达式语句 (i = 0;)
            // 注意：必须以分号结尾
            const expr = try self.parseExpression(.Lowest);
            _ = try self.expect(.Semicolon);

            // 将 Expression 包装成 ExpressionStatement
            const stmt = ast.Statement{ .ExpressionStatement = expr };
            initializer = try self.create(ast.Statement, stmt);
        }

        // === 2. 条件部分 (Condition) ===
        var condition: ?ast.Expression = null;

        // 如果不是分号，说明有条件表达式
        if (!self.check(.Semicolon)) {
            condition = try self.parseExpression(.Lowest);
        }
        _ = try self.expect(.Semicolon); // 强制分号

        // === 3. 步进部分 (Post Iteration) ===
        var post: ?ast.Expression = null;

        // 如果不是 {，说明有步进表达式
        // 步进部分后面紧跟 {，没有分号
        if (!self.check(.LBrace)) {
            post = try self.parseExpressionNoStruct(.Lowest);
        }

        // === 4. 循环体 (Body) ===
        const body_expr = try self.parseBlockExpression();

        const node = try self.create(ast.ForStatement, .{
            .initializer = initializer,
            .condition = condition,
            .post_iteration = post,
            .body = body_expr.Block,
            .span = start.span.merge(body_expr.span()),
        });
        return .{ .For = node };
    }

    /// 解析结构体初始化语法
    /// type_expr: 已经解析出来的类型部分 (通常是 Identifier 或 GenericInstantiation)
    /// start_brace: '{' token
    fn parseStructInitialization(self: *Parser, type_expr: ast.Expression) !ast.Expression {
        const allocator = self.ast_arena.allocator();
        // 确保下一个是 {
        _ = try self.expect(.LBrace);

        var fields = std.ArrayList(ast.StructFieldInit).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 字段名
            const name_tok = try self.expect(.Identifier);
            const name_sym = try self.internToken(name_tok);
            var value_expr: ast.Expression = undefined;

            // 检查冒号 field: value
            if (self.match(&.{.Colon})) {
                value_expr = try self.parseExpression(.Lowest);
            } else {
                // 简写 field (等同于 field: field)
                value_expr = .{ .Identifier = .{ .name = name_sym, .span = name_tok.span } };
            }

            try fields.append(allocator, .{
                .name = name_sym,
                .value = value_expr,
                .span = name_tok.span.merge(value_expr.span()),
            });

            if (!self.match(&.{.Comma})) break;
        }

        const end_brace = try self.expect(.RBrace);

        const node = try self.create(ast.StructInitializationExpression, .{
            .type_expression = type_expr,
            .fields = try fields.toOwnedSlice(allocator),
            .span = type_expr.span().merge(end_brace.span),
        });
        return .{ .StructInitialization = node };
    }

    // ==========================================
    // Top-Level Declarations
    // ==========================================

    /// [Helper] 解析可见性修饰符
    /// 仅仅消耗 'pub' (如果有的话)
    fn parseVisibility(self: *Parser) ast.Visibility {
        if (self.match(&.{.Pub})) {
            return .Public;
        }
        return .Private;
    }

    /// [Entry Point] 解析一个完整的声明 (包含可见性)
    /// 用于 Module 顶层或允许声明的 Statement 位置
    fn parseDeclaration(self: *Parser) ParseError!ast.Declaration {
        const vis = self.parseVisibility();
        return self.parseDeclarationRest(vis);
    }

    /// [Core Logic] 给定可见性，解析剩余的声明部分
    /// 此时 'pub' 已经被消耗，根据 peek() 的 Token 分发
    fn parseDeclarationRest(self: *Parser, vis: ast.Visibility) ParseError!ast.Declaration {
        const token = self.peek();
        switch (token.tag) {
            .Fn => return self.parseFunctionDeclaration(vis),
            .Struct => return self.parseStructDeclaration(vis),
            .Enum => return self.parseEnumDeclaration(vis),
            .Union => return self.parseUnionDeclaration(vis),
            .Trait => return self.parseTraitDeclaration(vis),
            .Impl => return self.parseImplDeclaration(),
            .Use => return self.parseUseDeclaration(vis),
            .Macro => return self.parseMacroDeclaration(vis),
            .Extern => return self.parseExternBlock(),
            .Type => return self.parseTypeAliasDeclaration(vis),

            // 静态变量 (Static / Const)
            .Static => return self.parseGlobalVarDeclaration(vis, .Static),
            .Const => return self.parseGlobalVarDeclaration(vis, .Const),

            // 错误处理
            else => {
                try self.errorAtCurrent(.UnexpectedToken);
                return error.ParseError;
            },
        }
    }

    fn parseGenericParameters(self: *Parser) ![]ast.GenericParameter {
        const allocator = self.ast_arena.allocator();
        // 如果不是 < 开头，说明没有泛型参数
        if (!self.check(.LessThan)) {
            return &.{};
        }

        _ = self.advance(); // eat <
        var params = std.ArrayList(ast.GenericParameter).empty;

        while (!self.check(.GreaterThan) and !self.check(.Eof)) {
            const name_tok = try self.expect(.Identifier);
            const sym = try self.internToken(name_tok);

            // 约束: T: Addable + Copy
            var constraints = std.ArrayList(ast.Expression).empty;
            if (self.match(&.{.Colon})) {
                // 解析第一个约束
                while (true) {
                    const constraint_type = try self.parseType();
                    try constraints.append(allocator, constraint_type);

                    // 如果有 + 号，继续解析下一个约束
                    if (!self.match(&.{.Plus})) break;
                }
            }

            // 默认值: T = i32 (0.0.5 新增)
            var default: ?ast.Expression = null;
            if (self.match(&.{.Assign})) {
                default = try self.parseExpression(.Lowest);
            }

            try params.append(allocator, .{
                .name = sym,
                .constraints = try constraints.toOwnedSlice(allocator), // fix: 赋值 slice
                .default_value = default,
                .span = name_tok.span,
            });

            if (!self.match(&.{.Comma})) break;
        }

        _ = try self.expect(.GreaterThan);
        return params.toOwnedSlice(allocator);
    }

    fn parseStructDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'struct'

        // 1. 名字
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型定义 <T>
        const generics = try self.parseGenericParameters();

        // 3. 继承语法 : BaseType
        var base_type: ?ast.Expression = null;
        if (self.match(&.{.Colon})) {
            base_type = try self.parseExpression(.Prefix);
        }

        // 4. 解析主体 { ... }
        _ = try self.expect(.LBrace);

        var fields = std.ArrayList(ast.StructFieldDeclaration).empty;
        var decls = std.ArrayList(ast.Declaration).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // A. 先解析可见性 (Common Step)
            const member_vis = self.parseVisibility();

            const token = self.peek();

            // B. 分支判断：是嵌套声明还是实例字段？
            switch (token.tag) {
                // === 情况 1: 嵌套声明 (Nested Declarations) ===
                // 只要是这些关键字开头，就调用 parseDeclarationRest
                .Fn, .Struct, .Enum, .Union, .Trait, .Impl, .Type, .Const, .Static, .Extern, .Use, .Macro => {
                    const decl = try self.parseDeclarationRest(member_vis);
                    try decls.append(allocator, decl);
                },

                // === 情况 2: 实例字段 (Identifier: Type) ===
                .Identifier => {
                    // 字段名
                    const field_name_tok = self.advance();
                    const field_sym = try self.internToken(field_name_tok);

                    _ = try self.expect(.Colon);
                    const field_type = try self.parseType();

                    // 默认值
                    var default_val: ?ast.Expression = null;
                    if (self.match(&.{.Assign})) {
                        default_val = try self.parseExpression(.Lowest);
                    }

                    // 逗号
                    _ = self.match(&.{.Comma});

                    try fields.append(allocator, .{
                        .name = field_sym,
                        .visibility = member_vis,
                        .type_expression = field_type,
                        .default_value = default_val,
                        .span = field_name_tok.span.merge(if (default_val) |v| v.span() else field_type.span()),
                    });
                },

                else => {
                    try self.errorAtCurrent(.UnexpectedToken);
                    // 错误恢复：跳过当前 Token 防止死循环
                    _ = self.advance();
                },
            }
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.StructDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .base_type = base_type,
            .fields = try fields.toOwnedSlice(allocator),
            .declarations = try decls.toOwnedSlice(allocator), // [Filled]
            .span = start.span.merge(end.span),
        });
        return .{ .Struct = node };
    }

    fn parseFunctionDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat fn

        // 1. 函数名
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型参数 (fn foo<T>)
        const generics = try self.parseGenericParameters();

        // 3. 参数列表
        _ = try self.expect(.LParen);
        var params = std.ArrayList(ast.FunctionParameter).empty;
        var is_variadic = false;

        if (!self.check(.RParen)) {
            while (true) {
                // 3.1 检查变长参数 ... (C FFI)
                if (self.match(&.{.Ellipsis})) {
                    is_variadic = true;
                    break;
                }

                // 3.2 self 参数处理 (必须是第一个参数)
                // 只有当 params 为空时才允许解析 self
                if (params.items.len == 0 and (self.check(.SelfValue) or self.check(.Ampersand))) {
                    // 尝试解析 self / &self / &mut self
                    // 如果解析成功，continue 继续下一次循环
                    if (try self.parseSelfParameter(&params)) {
                        if (!self.match(&.{.Comma})) break;
                        continue;
                    }
                    // 如果返回 false，说明是以 & 开头的普通参数 (如 &i32)，走下面的普通逻辑
                }

                // 3.3 普通参数: name: Type
                const param_name = try self.expect(.Identifier);
                const param_sym = try self.internToken(param_name);

                _ = try self.expect(.Colon);

                // Binding Cast: name: as Type
                // 检测是否存在 'as' 关键字
                var is_binding_cast = false;
                if (self.match(&.{.As})) {
                    is_binding_cast = true;
                }

                const param_type = try self.parseType();

                // 默认参数
                var default: ?ast.Expression = null;
                if (self.match(&.{.Assign})) {
                    default = try self.parseExpression(.Lowest);
                }

                try params.append(allocator, .{
                    .name = param_sym,
                    .type_expression = param_type,
                    .default_value = default,
                    .is_binding_cast = is_binding_cast,
                    .is_variadic = false,
                    .span = param_name.span.merge(if (default) |d| d.span() else param_type.span()),
                });

                if (!self.match(&.{.Comma})) break;
            }
        }
        _ = try self.expect(.RParen);

        // 4. 返回值
        var return_type: ?ast.Expression = null;
        if (!self.check(.LBrace) and !self.check(.Semicolon)) {
            // [修正] 必须用 parseType
            return_type = try self.parseType();
        }

        // 5. 函数体
        var body: ?*ast.BlockExpression = null;
        if (self.match(&.{.Semicolon})) {
            body = null;
        } else {
            const body_expr = try self.parseBlockExpression();
            body = body_expr.Block;
        }

        const end_span = if (body) |b| b.span else (if (return_type) |r| r.span() else name_tok.span);

        const node = try self.create(ast.FunctionDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .is_extern = false,
            .parameters = try params.toOwnedSlice(allocator),
            .return_type = return_type,
            .body = body,
            .span = start.span.merge(end_span),
        });
        return .{ .Function = node };
    }

    /// 尝试解析 self 参数。如果是 self 参数，返回 true 并添加到 params 中。
    /// 否则（比如是普通参数）返回 false。
    fn parseSelfParameter(self: *Parser, params: *std.ArrayList(ast.FunctionParameter)) !bool {
        const allocator = self.ast_arena.allocator();
        var is_ref = false;
        var is_mut = false;
        const start_span = self.peek().span;

        // case 1: &self 或 &mut self
        if (self.match(&.{.Ampersand})) {
            is_ref = true;
            // 检查 mut
            if (self.match(&.{.Mut})) {
                is_mut = true;
            }
        }

        // 必须紧跟 self 关键字
        if (!self.match(&.{.SelfValue})) {
            if (is_ref or is_mut) {
                try self.errorAtCurrent(.ExpectedIdentifier); // Expected 'self'
                return error.ParseError;
            }
            return false; // 没吃任何东西，不是 self 参数
        }

        const self_span = self.stream.prev_token_span;
        const full_span = start_span.merge(self_span);

        // 构造参数名 "self"
        const name_sym = try self.context.intern("self");

        // 构造类型: Self, &Self, or &mut Self
        // 1. 基础类型 Self
        const self_type_sym = try self.context.intern("Self");
        var type_expr: ast.Expression = .{ .Identifier = .{ .name = self_type_sym, .span = self_span } };

        // 2. 如果是引用，包裹一层 PointerType
        if (is_ref) {
            const ptr_node = try self.create(ast.PointerTypeExpression, .{
                .is_mutable = is_mut,
                .is_volatile = false,
                .child_type = type_expr,
                .span = full_span,
            });
            type_expr = .{ .PointerType = ptr_node };
        }

        try params.append(allocator, .{
            .name = name_sym,
            .type_expression = type_expr,
            .default_value = null,
            .is_binding_cast = false,
            .is_variadic = false,
            .span = full_span,
        });

        return true;
    }

    fn parseUseDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'use'

        // ==========================================
        // 1. 解析路径起点 (Base)
        // ==========================================
        var root_expr: ast.Expression = undefined;

        if (self.match(&.{.Dot})) {
            // 相对路径 .submod
            const sym = try self.context.intern(".");
            root_expr = .{ .Identifier = .{ .name = sym, .span = self.stream.prev_token_span } };
        } else if (self.match(&.{.DotDot})) {
            // 父级路径 ..utils
            const sym = try self.context.intern("..");
            root_expr = .{ .Identifier = .{ .name = sym, .span = self.stream.prev_token_span } };
        } else {
            // 绝对路径 std.debug
            const token = try self.expect(.Identifier);
            const sym = try self.internToken(token);
            root_expr = .{ .Identifier = .{ .name = sym, .span = token.span } };
        }

        // ==========================================
        // 2. 解析路径链 (Chain Loop)
        // ==========================================
        var current_path = root_expr;
        var is_glob = false;
        var is_group = false; // [新增] 标记是否是 group import

        while (self.match(&.{.Dot})) {
            // 2.1 检查 Glob: use xxx.*
            if (self.match(&.{.Star})) {
                is_glob = true;
                break; // Glob 必须是终点
            }

            // 2.2 检查 Group: use std.{a, b}
            if (self.check(.LBrace)) {
                is_group = true;
                _ = self.advance(); // eat {

                var members = std.ArrayList(ast.Expression).empty;
                while (!self.check(.RBrace) and !self.check(.Eof)) {
                    try members.append(allocator, try self.parseExpression(.Lowest));
                    if (!self.match(&.{.Comma})) break;
                }
                const group_end = try self.expect(.RBrace);

                const group_node = try self.create(ast.ImportGroupExpression, .{
                    .parent = current_path,
                    .sub_paths = try members.toOwnedSlice(allocator),
                    .span = current_path.span().merge(group_end.span),
                });

                current_path = .{ .ImportGroup = group_node };
                break; // Group 必须是终点
            }

            // 2.3 普通路径段: .debug
            const token = try self.expect(.Identifier);
            const sym = try self.internToken(token);

            const node = try self.create(ast.MemberAccessExpression, .{
                .object = current_path,
                .member_name = sym,
                .span = current_path.span().merge(token.span),
            });
            current_path = .{ .MemberAccess = node };
        }

        // ==========================================
        // 3. 别名 (Alias)
        // ==========================================
        var alias: ?SymbolId = null;

        // Glob (*) 和 Group ({}) 都不允许起别名
        if (!is_glob and !is_group and self.match(&.{.As})) {
            const alias_tok = try self.expect(.Identifier);
            alias = try self.internToken(alias_tok);
        }

        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.UseDeclaration, .{
            .visibility = vis,
            .path = current_path,
            .alias = alias,
            .is_glob = is_glob,
            .span = start.span.merge(end.span),
        });
        return .{ .Use = node };
    }

    fn parseTypeAliasDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const start = self.advance(); // eat 'type'

        // 1. 名字
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型参数 (可选)
        const generics = try self.parseGenericParameters();

        // 3. 等号
        _ = try self.expect(.Assign);

        // 4. 目标类型
        // 这里必须用 .Lowest，因为类型可能是复杂的表达式
        const target = try self.parseType();

        // 5. 分号
        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.TypeAliasDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .target = target,
            .span = start.span.merge(end.span),
        });

        return .{ .TypeAlias = node };
    }

    // 改名建议：parseStaticOrConstDeclaration
    fn parseGlobalVarDeclaration(self: *Parser, vis: ast.Visibility, start_kind: ast.GlobalVarKind) !ast.Declaration {
        const start = self.advance(); // eat 'const' or 'static'
        var kind = start_kind;

        // 1. 如果是 static，检查后面有没有 mut
        if (start.tag == .Static) {
            if (self.match(&.{.Mut})) {
                kind = .StaticMut;
            }
        }

        // 2. 名字 (强制 Identifier)
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 3. 类型注解 (static 必须有, const 可选)
        var type_anno: ?ast.Expression = null;
        if (self.match(&.{.Colon})) {
            type_anno = try self.parseType();
        } else if (kind != .Const) {
            // static 必须显式标注类型，Loom 设计偏好显式
            try self.errorAtCurrent(.ExpectedType);
        }

        // 4. 初始化值
        _ = try self.expect(.Assign);
        const value = try self.parseExpression(.Lowest);

        const end = try self.expect(.Semicolon);

        const node = try self.create(ast.GlobalVarDeclaration, .{
            .kind = kind,
            .visibility = vis,
            .name = name_sym, // AST 中已改为 name
            .type_annotation = type_anno,
            .value = value,
            .span = start.span.merge(end.span),
        });

        return .{ .GlobalVar = node };
    }

    fn parseUnionDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'union'

        // 1. 名字
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型 <T>
        const generics = try self.parseGenericParameters();

        // 3. 变体列表 { x: i32, y: f32 }
        _ = try self.expect(.LBrace);
        var variants = std.ArrayList(ast.UnionVariant).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            const var_name = try self.expect(.Identifier);
            const var_sym = try self.internToken(var_name);

            _ = try self.expect(.Colon);
            const type_expr = try self.parseExpression(.Lowest);

            // 允许尾随逗号
            _ = self.match(&.{.Comma});

            try variants.append(allocator, .{
                .name = var_sym,
                .type_expression = type_expr,
                .span = var_name.span.merge(type_expr.span()),
            });
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.UnionDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .variants = try variants.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Union = node };
    }

    fn parseEnumDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const start = self.advance(); // eat 'enum'

        // 1. 名字
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型参数 <T>
        const generics = try self.parseGenericParameters();

        // 3. 底层类型 (可选): enum Color: u8
        var underlying_type: ?ast.Expression = null;
        if (self.match(&.{.Colon})) {
            underlying_type = try self.parseType();
        }

        // 4. 变体列表 { ... }
        _ = try self.expect(.LBrace);
        var variants = std.ArrayList(ast.EnumVariant).empty;

        const allocator = self.ast_arena.allocator();

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 4.1 变体名字
            const var_name_tok = try self.expect(.Identifier);
            const var_sym = try self.internToken(var_name_tok);

            var kind: @TypeOf(variants.items[0].kind) = .None;
            var end_span = var_name_tok.span; // 默认结束位置是名字本身 (Unit Variant)

            // 4.2 判断变体类型
            if (self.check(.LBrace)) {
                // === Struct-like Variant: Message { x: i32 = 1 } ===
                _ = self.advance(); // eat {

                var fields = std.ArrayList(ast.StructFieldDeclaration).empty;

                while (!self.check(.RBrace) and !self.check(.Eof)) {
                    // Enum 变体字段默认 Public，但允许用户显式写 pub
                    var field_vis = ast.Visibility.Public; // 默认为 Public
                    if (self.match(&.{.Pub})) {
                        field_vis = .Public;
                    }
                    //? TODO: 如果未来想支持私有字段，可以检测 Private 关键字，但在 Enum 中很少见

                    const field_name_tok = try self.expect(.Identifier);
                    const field_sym = try self.internToken(field_name_tok);

                    _ = try self.expect(.Colon);
                    const field_type = try self.parseType();

                    var default_val: ?ast.Expression = null;
                    if (self.match(&.{.Assign})) {
                        default_val = try self.parseExpression(.Lowest);
                    }

                    try fields.append(allocator, .{
                        .name = field_sym,
                        .visibility = field_vis,
                        .type_expression = field_type,
                        .default_value = default_val, // 这里存入默认值
                        .span = field_name_tok.span.merge(if (default_val) |v| v.span() else field_type.span()),
                    });

                    if (!self.match(&.{.Comma})) break;
                }
                const rbrace = try self.expect(.RBrace);

                kind = .{ .StructLike = try fields.toOwnedSlice(allocator) };
                end_span = rbrace.span; // 修正 Span

            } else if (self.check(.LParen)) {
                // === Tuple-like Variant: Color(u8, u8) ===
                _ = self.advance(); // eat (

                var types = std.ArrayList(ast.Expression).empty;

                while (!self.check(.RParen) and !self.check(.Eof)) {
                    const type_expr = try self.parseType();
                    try types.append(allocator, type_expr);
                    if (!self.match(&.{.Comma})) break;
                }
                const rparen = try self.expect(.RParen);

                kind = .{ .TupleLike = try types.toOwnedSlice(allocator) };
                end_span = rparen.span; // 修正 Span

            } else if (self.match(&.{.Assign})) {
                // === Value Variant: Quit = 1 ===
                const val_expr = try self.parseExpression(.Lowest);
                kind = .{ .Value = val_expr };
                end_span = val_expr.span(); // 修正 Span
            }
            // else: Unit Variant (end_span 已经在上面初始化为 name_tok.span 了)

            // 4.3 构造并添加 Variant
            const full_span = var_name_tok.span.merge(end_span);

            try variants.append(allocator, .{
                .name = var_sym,
                .kind = kind,
                .span = full_span,
            });

            _ = self.match(&.{.Comma});
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.EnumDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .underlying_type = underlying_type,
            .variants = try variants.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Enum = node };
    }

    fn parseTraitDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'trait'

        // 1. 名字
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        // 2. 泛型 <T>
        const generics = try self.parseGenericParameters();

        // 3. 解析父特质 (Super Traits)
        // 语法: trait Child: Parent + Copy { ... }
        var super_traits = std.ArrayList(ast.Expression).empty;

        // 检查是否有冒号
        if (self.match(&.{.Colon})) {
            while (true) {
                // 解析类型 (Trait 本质上是类型表达式)
                // parseType 会解析 Identifier 或 Path (std.io.Reader) 或 Generic (Iterable<T>)
                // 它不会吃掉 + 号，因为 parseType 遇到 + 会停止
                const trait_expr = try self.parseType();
                try super_traits.append(allocator, trait_expr);

                // 如果没有 + 号，说明列表结束
                if (!self.match(&.{.Plus})) break;
            }
        }

        // 4. 方法列表 { fn ... }
        _ = try self.expect(.LBrace);
        var methods = std.ArrayList(ast.FunctionDeclaration).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 检查是否是 fn 开头
            if (self.check(.Fn) or self.check(.Pub)) {
                // 如果有 pub 修饰符，需要先解析
                var method_vis = ast.Visibility.Private;
                if (self.match(&.{.Pub})) method_vis = .Public;

                // 确保是 fn
                if (self.check(.Fn)) {
                    const decl = try self.parseFunctionDeclaration(method_vis);
                    // 解包 Declaration.Function
                    if (decl == .Function) {
                        try methods.append(allocator, decl.Function.*); // copy struct value
                    } else {
                        return error.ParseError;
                    }
                } else {
                    try self.errorAtCurrent(.UnexpectedToken);
                }
            } else {
                // 遇到非 fn token，可能是结束了或者错误
                if (self.check(.RBrace)) break;
                try self.errorAtCurrent(.UnexpectedToken);
                _ = self.advance(); // 避免死循环
            }
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.TraitDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .generics = generics,
            .super_traits = try super_traits.toOwnedSlice(allocator),
            .methods = try methods.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Trait = node };
    }

    fn parseImplDeclaration(self: *Parser) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'impl'

        // 1. 解析泛型参数
        const generics = try self.parseGenericParameters();

        // 2. 解析目标类型
        const target_type = try self.parseType();

        // 3. 解析 Trait 接口 (可选)
        var trait_interface: ?ast.Expression = null;
        if (self.match(&.{.Colon})) {
            trait_interface = try self.parseType();
        }

        // 4. 解析声明列表
        _ = try self.expect(.LBrace);

        var decls = std.ArrayList(ast.Declaration).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // A. 解析可见性
            const member_vis = self.parseVisibility();

            // B. 检查禁止嵌套 Impl
            if (self.check(.Impl)) {
                // 明确禁止嵌套 impl
                try self.errorAtCurrent(.UnexpectedToken);
                // 错误恢复：吃掉 impl 防止死循环
                _ = self.advance();
                continue;
            }

            // C. 检查允许的 Token
            // 允许: fn, const, static, struct, enum, union, type, use, macro...
            // 这些都在 parseDeclarationRest 中处理了
            const token = self.peek();

            // 白名单检查
            switch (token.tag) {
                .Fn, .Const, .Static, .Struct, .Enum, .Union, .Type, .Use, .Macro, .Extern => {
                    // 复用通用的声明解析逻辑
                    const decl = try self.parseDeclarationRest(member_vis);
                    try decls.append(allocator, decl);
                },
                else => {
                    // 遇到无法识别的 Token
                    try self.errorAtCurrent(.UnexpectedToken);
                    _ = self.advance();
                },
            }
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.ImplementationDeclaration, .{
            .generics = generics,
            .target_type = target_type,
            .trait_interface = trait_interface,
            .declarations = try decls.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Implementation = node };
    }

    fn parseExternBlock(self: *Parser) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'extern'

        // extern { fn ... }
        _ = try self.expect(.LBrace);

        var decls = std.ArrayList(ast.Declaration).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            const decl = try self.parseDeclaration();

            // 可以在这里加校验：只允许 Function 和 GlobalVar
            switch (decl) {
                .Function, .GlobalVar => try decls.append(allocator, decl),
                else => {
                    // 报错：Extern 块里只能有函数或变量
                    try self.errorAtCurrent(.UnexpectedToken);
                },
            }
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.ExternBlockDeclaration, .{
            .declarations = try decls.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .ExternBlock = node };
    }

    /// 解析宏片段说明符
    /// input: 当前 token 应为 Identifier (例如 "expr", "ident")
    fn parseMacroFragmentSpecifier(self: *Parser) !ast.MacroFragmentSpecifier {
        const token = try self.expect(.Identifier);
        const text = token.span.slice(self.source);

        if (std.mem.eql(u8, text, "expr")) return .Expression;
        if (std.mem.eql(u8, text, "ident")) return .Identifier;
        if (std.mem.eql(u8, text, "type") or std.mem.eql(u8, text, "ty")) return .Type;
        if (std.mem.eql(u8, text, "stmt")) return .Statement;
        if (std.mem.eql(u8, text, "block")) return .Block;
        if (std.mem.eql(u8, text, "path")) return .Path;
        if (std.mem.eql(u8, text, "literal")) return .Literal;
        if (std.mem.eql(u8, text, "tt")) return .TokenTree;

        // 如果拼写错误，抛出具体的错误
        // 这里简单返回 ParseError，实际应当 errorAtCurrent("Unknown fragment specifier")
        try self.errorAtCurrent(.UnexpectedToken);
        return error.ParseError;
    }

    /// 递归解析宏匹配器序列
    /// end_token: 期望的结束符 (通常是 RParen)
    fn parseMacroMatchers(self: *Parser, end_token: TokenType) ![]ast.MacroMatcher {
        const allocator = self.ast_arena.allocator();
        var matchers = std.ArrayList(ast.MacroMatcher).empty;

        while (!self.check(end_token) and !self.check(.Eof)) {
            // Case A: 遇到 $ (可能是 Capture 或 Repetition)
            if (self.check(.Dollar)) {
                // 向前看一个 Token 决定是 $name 还是 $(
                const next_tok = self.stream.peek(1);

                if (next_tok.tag == .LParen) {
                    // === Case A.1: 重复模式 $(...) ===
                    const start_span = self.advance().span; // eat $
                    _ = self.advance(); // eat (

                    // 1. 递归解析括号内部
                    const sub_matchers = try self.parseMacroMatchers(.RParen);

                    _ = try self.expect(.RParen); // eat )

                    // 2. 解析分隔符 (Separator) 和 操作符 (Op)
                    // 语法可能性:
                    // 1. $(...)* -> sep=null, op=*
                    // 2. $(...),* -> sep=,,    op=*
                    // 3. $(...)+   -> sep=null, op=+

                    var separator: ?Token = null;
                    var op: ast.MacroRepetitionOp = undefined;

                    // 检查当前是否直接是操作符
                    if (self.check(.Star)) {
                        op = .ZeroOrMore;
                        _ = self.advance();
                    } else if (self.check(.Plus)) {
                        op = .OneOrMore;
                        _ = self.advance();
                    } else if (self.check(.Question)) {
                        op = .ZeroOrOne;
                        _ = self.advance();
                    } else {
                        // 如果不是操作符，那当前 Token 必须是分隔符
                        separator = self.advance();

                        // 分隔符后面必须紧跟操作符
                        if (self.check(.Star)) {
                            op = .ZeroOrMore;
                            _ = self.advance();
                        } else if (self.check(.Plus)) {
                            op = .OneOrMore;
                            _ = self.advance();
                        } else if (self.check(.Question)) {
                            op = .ZeroOrOne;
                            _ = self.advance();
                        } else {
                            // 既不是操作符，也不是带操作符的分隔符 -> 语法错误
                            // 宏定义中 $(...) 后面必须跟重复操作符
                            try self.errorAtCurrent(.UnexpectedToken);
                            return error.ParseError;
                        }
                    }

                    const end_span = self.stream.prev_token_span;

                    try matchers.append(allocator, .{ .Repetition = .{
                        .matchers = sub_matchers,
                        .separator = separator,
                        .op = op,
                        .span = start_span.merge(end_span),
                    } });
                } else {
                    // === Case A.2: 参数捕获 $name:spec ===
                    const dollar_span = self.advance().span; // eat $
                    const name_tok = try self.expect(.Identifier);
                    const name_sym = try self.internToken(name_tok);

                    _ = try self.expect(.Colon);
                    const spec = try self.parseMacroFragmentSpecifier();

                    const span = dollar_span.merge(self.stream.prev_token_span);
                    try matchers.append(allocator, .{ .Argument = .{
                        .name = name_sym,
                        .fragment = spec,
                        .span = span,
                    } });
                }
            }
            // Case B: 字面量
            else {
                const tok = self.advance();
                try matchers.append(allocator, .{ .Literal = tok });
            }
        }

        return matchers.toOwnedSlice(allocator);
    }

    // 解析宏的 Body (Token Tree)
    // 遇到 { 开头，直到匹配的 } 结束
    fn parseMacroBody(self: *Parser) ![]const Token {
        const allocator = self.ast_arena.allocator();
        const start_token = try self.expect(.LBrace); // eat {

        var tokens = std.ArrayList(Token).empty;
        // 把起始的 { 加进去
        try tokens.append(allocator, start_token);

        var nesting: usize = 1;

        while (nesting > 0 and !self.check(.Eof)) {
            const t = self.advance();
            try tokens.append(allocator, t);

            if (t.tag == .LBrace) {
                nesting += 1;
            } else if (t.tag == .RBrace) {
                nesting -= 1;
            }
        }

        if (nesting > 0) {
            try self.errorAtCurrent(.UnexpectedToken); // Unclosed macro body
            return error.ParseError;
        }

        return tokens.toOwnedSlice(allocator);
    }

    fn parseMacroDeclaration(self: *Parser, vis: ast.Visibility) !ast.Declaration {
        const allocator = self.ast_arena.allocator();
        const start = self.advance(); // eat 'macro'
        const name_tok = try self.expect(.Identifier);
        const name_sym = try self.internToken(name_tok);

        _ = try self.expect(.LBrace);
        var rules = std.ArrayList(ast.MacroRule).empty;

        while (!self.check(.RBrace) and !self.check(.Eof)) {
            // 1. Matchers
            _ = try self.expect(.LParen);
            const matchers = try self.parseMacroMatchers(.RParen);
            _ = try self.expect(.RParen);

            // 2. Arrow =>
            _ = try self.expect(.Arrow);

            // 3. Body
            const body_tokens = try self.parseMacroBody();

            // 计算 Span: 从 name 到 body 的最后一个 token
            const last_token = body_tokens[body_tokens.len - 1];
            const rule_span = name_tok.span.merge(last_token.span);

            // 4. Delimiter (可选的分号)
            _ = self.match(&.{ .Semicolon, .Comma });

            try rules.append(allocator, .{
                .matchers = matchers,
                .body = body_tokens,
                .span = rule_span,
            });
        }

        const end = try self.expect(.RBrace);

        const node = try self.create(ast.MacroDeclaration, .{
            .name = name_sym,
            .visibility = vis,
            .rules = try rules.toOwnedSlice(allocator),
            .span = start.span.merge(end.span),
        });
        return .{ .Macro = node };
    }

    // ==========================================
    // Root Entry
    // ==========================================

    pub fn parseModule(self: *Parser) !ast.Module {
        const allocator = self.ast_arena.allocator();
        var decls = std.ArrayList(ast.Declaration).empty;

        while (!self.check(.Eof)) {
            // 跳过多余的分号（如果有的话）
            if (self.match(&.{.Semicolon})) continue;

            const decl = try self.parseDeclaration();
            try decls.append(allocator, decl);
        }

        return ast.Module{
            .declarations = try decls.toOwnedSlice(allocator),
        };
    }

    // --- Helpers -----------------------------

    /// 辅助：创建一个单元素的切片
    fn singleElementSlice(self: *Parser, expr: ast.Expression) ![]ast.Expression {
        const slice = try self.ast_arena.allocator().alloc(ast.Expression, 1);
        slice[0] = expr;
        return slice;
    }

    /// 启发式判断：Token 是否看起来像一个类型的开头
    fn isTypeStart(token: Token) bool {
        return switch (token.tag) {
            // === 肯定不是类型 ===
            .IntLiteral,
            .FloatLiteral,
            .CharLiteral,
            .StringLiteral,
            .True,
            .False,
            .Null,
            .Undef,
            .Unreach,
            => false,

            // === 肯定是类型开头 ===
            .Identifier, // MyType
            .Fn, // fn(i32)
            .Question, // ?T
            .Ampersand, // &T
            .Star, // *T
            .LBracket, // []T, [N]T
            .Bang, // ! (Never Type)
            .SelfType, // Self
            .LParen, // (i32, i32) 元组类型
            .Struct,
            .Enum,
            .Union,
            => false,

            else => false,
        };
    }
};

// ==========================================
// 测试
// ==========================================

const Lexer = @import("lexer.zig").Lexer;
const testing = std.testing;

const TestContext = struct {
    // 指针指向堆上的 Arena，确保地址不变
    arena_ptr: *std.heap.ArenaAllocator,
    context: *Context,
    parser: Parser,

    pub fn init(source: []const u8) TestContext {
        // 1. 在堆上分配 Arena 实例
        const arena_ptr = testing.allocator.create(std.heap.ArenaAllocator) catch unreachable;
        // 初始化
        arena_ptr.* = std.heap.ArenaAllocator.init(testing.allocator);

        // 2. 现在获取 allocator 是安全的，因为 arena_ptr 指向的堆内存不会动
        const allocator = arena_ptr.allocator();

        // 3. 初始化 Context
        const ctx_ptr = allocator.create(Context) catch unreachable;
        ctx_ptr.* = Context.init(allocator);

        const lexer = Lexer.init(source);

        const stream = TokenStream.init(lexer);

        // 4. 初始化 Parser
        const parser = Parser.init(allocator, stream, ctx_ptr, source);

        return .{
            .arena_ptr = arena_ptr,
            .context = ctx_ptr,
            .parser = parser,
        };
    }

    pub fn deinit(self: *TestContext) void {
        self.parser.deinit();
        self.arena_ptr.deinit(); // 释放 arena 管理的所有内存
        testing.allocator.destroy(self.arena_ptr); // 释放 arena 结构体本身
    }

    pub fn parse(self: *TestContext) !ast.Module {
        return self.parser.parseModule();
    }
};

test "Parser: Macro Call Suffix Logic" {
    const source =
        \\fn main() {
        \\    std.debug.print!("Hello");
        \\    let v = vec![1, 2, 3];
        \\}
    ;

    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();

    // 检查第一个语句: std.debug.print!("Hello");
    const func = mod.declarations[0].Function;
    const stmt = func.body.?.statements[0];

    switch (stmt.ExpressionStatement) {
        .MacroCall => |call| {
            // 验证 Callee: std.debug.print
            switch (call.callee) {
                .MemberAccess => |ma| {
                    const name_str = tc.parser.context.resolve(ma.member_name);
                    try testing.expectEqualStrings("print", name_str);
                },
                else => try testing.expect(false),
            }

            // 验证参数: "Hello"
            try testing.expectEqual(@as(usize, 1), call.arguments.len);
            try testing.expectEqual(call.arguments[0].tag, .StringLiteral);
        },
        else => try testing.expect(false),
    }
}

test "Parser: Type Parsing (Generics without Turbofish)" {
    const source =
        \\type MyList = List<i32>;
        \\fn make() List<u8>; 
    ;

    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();

    // 1. 检查 Type Alias: List<i32>
    const type_alias = mod.declarations[0].TypeAlias;
    switch (type_alias.target) {
        .GenericInstantiation => |gen| {
            try testing.expectEqual(@as(usize, 1), gen.arguments.len);
            // 进一步验证参数是不是 i32 标识符
            // ...
        },
        else => try testing.expect(false),
    }

    // 2. 检查函数返回值: List<u8>
    const func_decl = mod.declarations[1].Function;
    const ret_type = func_decl.return_type.?;
    switch (ret_type) {
        .GenericInstantiation => |gen| {
            try testing.expectEqual(@as(usize, 1), gen.arguments.len);
        },
        else => try testing.expect(false),
    }
}

test "Parser: Use Group Import" {
    const source = "use std.io.{Read, Write};";
    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();
    const use_decl = mod.declarations[0].Use;

    switch (use_decl.path) {
        .ImportGroup => |group| {
            // Parent: std.io
            switch (group.parent) {
                .MemberAccess => {}, // OK
                else => try testing.expect(false),
            }
            // SubPaths: [Read, Write]
            try testing.expectEqual(@as(usize, 2), group.sub_paths.len);
        },
        else => try testing.expect(false),
    }
}

test "Parser: Strict For Loop" {
    const source =
        \\fn loop() {
        \\    for let mut i = 0; i < 10; i += 1 {
        \\        continue;
        \\    }
        \\}
    ;
    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();
    const stmt = mod.declarations[0].Function.body.?.statements[0];

    switch (stmt.For) {
        else => |f| {
            try testing.expect(f.initializer != null);
            try testing.expect(f.condition != null);
            try testing.expect(f.post_iteration != null);
        },
    }
}

test "Parser: Struct Pattern with Rest" {
    const source =
        \\fn main() {
        \\    let Point { x, .. } = p;
        \\}
    ;
    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();
    const stmt = mod.declarations[0].Function.body.?.statements[0];

    switch (stmt.Let.pattern) {
        .StructDestructuring => |sd| {
            // 验证字段数量和 .. 标志
            try testing.expectEqual(@as(usize, 1), sd.fields.len);
            try testing.expect(sd.ignore_remaining == true);
        },
        else => try testing.expect(false),
    }
}

test "Parser: Nested Declarations (Local Static & Struct)" {
    // 测试点：Statement 现在可以包含 Declaration
    // 我们在函数内部定义 static 变量和 struct
    const source =
        \\fn main() {
        \\    static counter: i32 = 0;
        \\    struct Inner { val: u8 }
        \\    let x = Inner { val: 1 };
        \\}
    ;
    var tc = TestContext.init(source);
    defer tc.deinit();

    const mod = try tc.parse();
    const body = mod.declarations[0].Function.body.?;

    // 语句 1: static counter (Declaration)
    const stmt1 = body.statements[0];
    switch (stmt1) {
        .Declaration => |d| {
            try testing.expectEqual(ast.GlobalVarKind.Static, d.GlobalVar.kind);
            // 验证名字是 counter
            const name = tc.parser.context.resolve(d.GlobalVar.name);
            try testing.expectEqualStrings("counter", name);
        },
        else => try testing.expect(false),
    }

    // 语句 2: struct Inner (Declaration)
    const stmt2 = body.statements[1];
    switch (stmt2) {
        .Declaration => |d| {
            switch (d) {
                .Struct => |s| {
                    const name = tc.parser.context.resolve(s.name);
                    try testing.expectEqualStrings("Inner", name);
                },
                else => try testing.expect(false),
            }
        },
        else => try testing.expect(false),
    }

    // 语句 3: let x (LetStatement)
    const stmt3 = body.statements[2];
    try testing.expect(stmt3 == .Let);
}

test "Parser: Static Mut and Const (Global Scope)" {
    const source =
        \\const PI: f32 = 3.14;
        \\static mut GlobalCounter: i32 = 0;
    ;
    var tc = TestContext.init(source);
    defer tc.deinit();
    const mod = try tc.parse();

    // 1. const PI
    const d1 = mod.declarations[0].GlobalVar;
    try testing.expectEqual(ast.GlobalVarKind.Const, d1.kind);

    // 2. static mut GlobalCounter
    const d2 = mod.declarations[1].GlobalVar;
    try testing.expectEqual(ast.GlobalVarKind.StaticMut, d2.kind);
}

test "Parser: Block Result Expression (No Semicolon)" {
    const source =
        \\fn get_val() i32 {
        \\    let a = 1;
        \\    a + 1 // 没有分号，作为返回值
        \\}
    ;
    var tc = TestContext.init(source);
    defer tc.deinit();
    const mod = try tc.parse();

    const body = mod.declarations[0].Function.body.?;

    // 验证 statements 只有 let a = 1
    try testing.expectEqual(@as(usize, 1), body.statements.len);

    // 验证 result_expression 存在且是 BinaryExpression
    try testing.expect(body.result_expression != null);
    switch (body.result_expression.?) {
        .Binary => {},
        else => try testing.expect(false),
    }
}

test "Parser: Inclusive Range" {
    const source =
        \\fn main() {
        \\    // 1. Range 作为表达式 (Expression)
        \\    let r = 0..=255; 
        \\
        \\    // 2. Range 作为模式 (Pattern)
        \\    match x {
        \\        'a'..='z' => {},
        \\        _ => {}
        \\    }
        \\}
    ;

    var tc = TestContext.init(source);
    defer tc.deinit();
    const mod = try tc.parse();

    const func_body = mod.declarations[0].Function.body.?;

    // --- 验证 1: let r = 0..=255; ---
    const let_stmt = func_body.statements[0].Let;
    switch (let_stmt.value) {
        .Range => |r| {
            try testing.expect(r.is_inclusive == true);

            // 验证 Start
            try testing.expectEqual(r.start.?.Literal.kind, .Integer);
            const start_val = tc.parser.context.resolve(r.start.?.Literal.value);
            try testing.expectEqualStrings("0", start_val);

            // 验证 End
            try testing.expectEqual(r.end.?.Literal.kind, .Integer);
            const end_val = tc.parser.context.resolve(r.end.?.Literal.value);
            try testing.expectEqualStrings("255", end_val);
        },
        else => try testing.expect(false),
    }

    // --- 验证 2: match ... 'a'..='z' ---
    // 检查 result_expression 是否存在
    try testing.expect(func_body.result_expression != null);

    // 获取表达式
    const match_expr = func_body.result_expression.?;

    switch (match_expr) {
        .Match => |m| {
            const arm = m.arms[0];
            switch (arm.pattern) {
                .Range => |r| {
                    try testing.expect(r.is_inclusive == true);
                    // 验证 Start 'a'
                    try testing.expectEqual(r.start.kind, .Character);
                    // 验证 End 'z'
                    try testing.expectEqual(r.end.kind, .Character);
                },
                else => try testing.expect(false),
            }
        },
        else => try testing.expect(false),
    }
}
