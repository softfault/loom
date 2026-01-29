const std = @import("std");
const Token = @import("token.zig").Token;
const TokenType = @import("token.zig").TokenType;
const Span = @import("utils.zig").Span;
const SymbolId = @import("utils.zig").SymbolId;

pub const NodeId = enum(u32) {
    _,

    pub fn toUsize(self: NodeId) usize {
        return @intFromEnum(self);
    }
};

/// 二元运算符
pub const BinaryOperator = enum {
    // 算术
    Add, // +
    Subtract, // -
    Multiply, // *
    Divide, // /
    Modulo, // %

    // 比较
    Equal, // ==
    NotEqual, // !=
    LessThan, // <
    GreaterThan, // >
    LessOrEqual, // <=
    GreaterOrEqual, // >=

    // 逻辑
    LogicalAnd, // and
    LogicalOr, // or

    // 位运算
    BitwiseAnd, // &
    BitwiseOr, // |
    BitwiseXor, // ^
    ShiftLeft, // <<
    ShiftRight, // >>

    NullCoalesce, // ?

    pub fn fromToken(token: TokenType) BinaryOperator {
        return switch (token) {
            .Plus => .Add,
            .Minus => .Subtract,
            .Star => .Multiply,
            .Slash => .Divide,
            .Percent => .Modulo,
            .Equal => .Equal,
            .NotEqual => .NotEqual,
            .LessThan => .LessThan,
            .GreaterThan => .GreaterThan,
            .LessEqual => .LessOrEqual,
            .GreaterEqual => .GreaterOrEqual,
            .And => .LogicalAnd,
            .Or => .LogicalOr,
            .Ampersand => .BitwiseAnd,
            .Pipe => .BitwiseOr,
            .Caret => .BitwiseXor,
            .LShift => .ShiftLeft,
            .RShift => .ShiftRight,
            .Question => .NullCoalesce,
            else => unreachable, // Parser 逻辑保证了只会传合法的 token 进来
        };
    }
};

/// 一元运算符
pub const UnaryOperator = enum {
    Negate, // - (负号)
    LogicalNot, // ! (逻辑非)
    BitwiseNot, // ~ (按位取反)
    AddressOf, // & (取地址)
    Dereference, // .& (解引用)
    Optional, // ? (判空)
    LengthOf, // # (取长度)
};

/// 赋值运算符
pub const AssignmentOperator = enum {
    Assign, // =
    AddAssign, // +=
    SubtractAssign, // -=
    MultiplyAssign, // *=
    DivideAssign, // /=
    ModuloAssign, // %=
    BitwiseAndAssign, // &=
    BitwiseOrAssign, // |=
    BitwiseXorAssign, // ^=
    ShiftLeftAssign, // <<=
    ShiftRightAssign, // >>=

    pub fn fromToken(token: TokenType) AssignmentOperator {
        return switch (token) {
            .Assign => .Assign,
            .PlusAssign => .AddAssign,
            .MinusAssign => .SubtractAssign,
            .StarAssign => .MultiplyAssign,
            .SlashAssign => .DivideAssign,
            .PercentAssign => .ModuloAssign,
            .AmpersandAssign => .BitwiseAndAssign,
            .PipeAssign => .BitwiseOrAssign,
            .CaretAssign => .BitwiseXorAssign,
            .LShiftAssign => .ShiftLeftAssign,
            .RShiftAssign => .ShiftRightAssign,
            else => unreachable, // 同上
        };
    }
};

/// 表达式
/// Loom 中类型也是表达式，所以 [4]i32 这种类型构造也在这里
pub const Expression = union(enum) {
    // 基础值
    Literal: Literal,
    Identifier: Identifier,

    // 运算
    Binary: *BinaryExpression,
    Unary: *UnaryExpression,
    Assignment: *AssignmentExpression,

    // 访问与调用
    FunctionCall: *FunctionCallExpression, // foo(1, 2) 或 Point(i32)
    // 泛型实例化: List<i32> (类型上下文) 或 func.<i32> (表达式上下文)
    GenericInstantiation: *GenericInstantiationExpression,
    MemberAccess: *MemberAccessExpression, // obj.field
    IndexAccess: *IndexAccessExpression, // arr[index]
    Propagate: *PropagateExpression, // result?
    // 宏调用表达式
    MacroCall: *MacroCallExpression,

    // 专门用于 use 语句的组导入
    // use std.debug.{print, assert};
    ImportGroup: *ImportGroupExpression,

    // 复合字面量
    StructInitialization: *StructInitializationExpression, // Point { x: 1 }
    ArrayInitialization: *ArrayInitializationExpression, // [1, 2, 3]
    TupleInitialization: *TupleInitializationExpression, // (1, "a")
    Range: *RangeExpression, // 1..100

    // 控制流表达式
    If: *IfExpression,
    Match: *MatchExpression,
    Block: *BlockExpression, // { stmt; stmt; expr }

    // 类型构造表达式
    PointerType: *PointerTypeExpression, // &T, *mut T
    SliceType: *SliceTypeExpression, // []T
    ArrayType: *ArrayTypeExpression, // [N]T
    OptionalType: *OptionalTypeExpression, // ?T
    FunctionType: *FunctionTypeExpression, // fn(i32) i32
    NeverType: *NeverTypeExpression, // !

    /// 获取表达式的 Span
    pub fn span(self: Expression) Span {
        return switch (self) {
            .Literal => |v| v.span,
            .Identifier => |v| v.span,
            .Binary => |v| v.span,
            .Unary => |v| v.span,
            .Assignment => |v| v.span,
            .FunctionCall => |v| v.span,
            .GenericInstantiation => |v| v.span,
            .MemberAccess => |v| v.span,
            .IndexAccess => |v| v.span,
            .ImportGroup => |v| v.span,
            .Propagate => |v| v.span,
            .MacroCall => |v| v.span,
            .StructInitialization => |v| v.span,
            .ArrayInitialization => |v| v.span,
            .TupleInitialization => |v| v.span,
            .Range => |v| v.span,
            .If => |v| v.span,
            .Match => |v| v.span,
            .Block => |v| v.span,
            .PointerType => |v| v.span,
            .SliceType => |v| v.span,
            .ArrayType => |v| v.span,
            .OptionalType => |v| v.span,
            .FunctionType => |v| v.span,
            .NeverType => |v| v.span,
        };
    }

    /// 获取表达式的ID
    pub fn id(self: Expression) NodeId {
        return switch (self) {
            .Literal => |v| v.id,
            .Identifier => |v| v.id,
            .Binary => |v| v.id,
            .Unary => |v| v.id,
            .Assignment => |v| v.id,
            .FunctionCall => |v| v.id,
            .GenericInstantiation => |v| v.id,
            .MemberAccess => |v| v.id,
            .IndexAccess => |v| v.id,
            .ImportGroup => |v| v.id,
            .Propagate => |v| v.id,
            .MacroCall => |v| v.id,
            .StructInitialization => |v| v.id,
            .ArrayInitialization => |v| v.id,
            .TupleInitialization => |v| v.id,
            .Range => |v| v.id,
            .If => |v| v.id,
            .Match => |v| v.id,
            .Block => |v| v.id,
            .PointerType => |v| v.id,
            .SliceType => |v| v.id,
            .ArrayType => |v| v.id,
            .OptionalType => |v| v.id,
            .FunctionType => |v| v.id,
            .NeverType => |v| v.id,
        };
    }
};

// === 具体的表达式结构体 ===

pub const Literal = struct {
    pub const Kind = enum {
        Integer,
        Float,
        String,
        Character,
        Boolean,
        Undef, // undef 关键字
        Null,
        Unreachable,
    };
    id: NodeId,
    kind: Kind,
    value: SymbolId, // 存储字符串化的值，留待语义分析阶段解析为数字
    span: Span,
};

pub const Identifier = struct {
    id: NodeId,
    name: SymbolId,
    span: Span,
};

pub const BinaryExpression = struct {
    id: NodeId,
    operator: BinaryOperator,
    left: Expression,
    right: Expression,
    span: Span,
};

pub const UnaryExpression = struct {
    id: NodeId,
    operator: UnaryOperator,
    operand: Expression,
    span: Span,
};

pub const AssignmentExpression = struct {
    id: NodeId,
    operator: AssignmentOperator,
    target: Expression, // 左值 (L-Value)
    value: Expression, // 右值 (R-Value)
    span: Span,
};

pub const FunctionCallExpression = struct {
    id: NodeId,
    callee: Expression, // 被调用的对象 (函数名、泛型类型名、函数指针等)
    arguments: []CallArgument,
    span: Span,
};

pub const CallArgument = struct {
    id: NodeId,
    name: ?SymbolId, // 如果是位置参数则为 null，如果是命名参数则为名字
    value: Expression,
    span: Span,
};

/// 泛型实例化表达式
/// 涵盖: Type<Args> (如 List<i32>) 和 Expr.<Args> (如 parse.<i32>)
pub const GenericInstantiationExpression = struct {
    id: NodeId,
    base: Expression, // 左边的部分，如 List 或 parse
    arguments: []Expression, // <...> 里面的参数，通常是类型表达式
    span: Span,
};

pub const MemberAccessExpression = struct {
    id: NodeId,
    object: Expression,
    member_name: SymbolId,
    span: Span,
};

pub const IndexAccessExpression = struct {
    id: NodeId,
    collection: Expression, // 数组、切片
    index: Expression, // 索引值
    span: Span,
};

/// 解包表达式
/// 对应语法: expression.?
pub const UnwrapExpression = struct {
    id: NodeId,
    operand: Expression,
    span: Span,
};
/// 传播表达式
/// result?
pub const PropagateExpression = struct {
    id: NodeId,
    operand: Expression,
    span: Span,
};

/// 宏调用表达式
/// 例如: vec![1, 2, 3] 或 std.debug.print!("fmt")
pub const MacroCallExpression = struct {
    id: NodeId,
    /// 被调用的宏 (通常是 Identifier 或 MemberAccess)
    /// 例如: "vec" 或 "std.debug.print"
    callee: Expression,

    /// 宏的参数 (Token Tree)
    /// 宏调用在语法分析阶段不解析参数内部结构，只保存原始 Token 序列
    /// 具体的解析留给宏展开器 (Expander)去做
    arguments: []const Token,

    span: Span,
};

// 导入组定义
pub const ImportGroupExpression = struct {
    id: NodeId,
    parent: Expression, // std.debug
    sub_paths: []Expression, // [print, assert] (通常是 Identifier，但也允许子路径)
    span: Span,
};

pub const StructInitializationExpression = struct {
    id: NodeId,
    // 可能是 null (如果是匿名结构体或上下文推导)
    // 对于 `Point { x: 1 }`, 这里是 `Identifier(Point)`
    type_expression: ?Expression,
    fields: []StructFieldInit,
    span: Span,
};

pub const StructFieldInit = struct {
    id: NodeId,
    name: SymbolId,
    value: Expression,
    span: Span,
};

pub const ArrayInitializationExpression = struct {
    id: NodeId,
    elements: []Expression,
    // 如果是 [0; 1024] 这种语法
    repeat_count: ?Expression,
    span: Span,
};

pub const TupleInitializationExpression = struct {
    id: NodeId,
    elements: []Expression,
    span: Span,
};

pub const RangeExpression = struct {
    id: NodeId,
    start: ?Expression, // null 表示 ..5 中的 start (0)
    end: ?Expression, // null 表示 1.. 中的 end (len)
    is_inclusive: bool,
    span: Span,
};

pub const IfExpression = struct {
    id: NodeId,
    condition: Expression,
    then_branch: Expression,
    else_branch: ?Expression, // 可能是 BlockExpression 或另一个 IfExpression (else if)
    span: Span,
};

pub const MatchExpression = struct {
    id: NodeId,
    target: Expression,
    arms: []MatchArm,
    span: Span,
};

pub const MatchArm = struct {
    id: NodeId,
    pattern: Pattern,
    body: Expression,
    span: Span,
};

pub const BlockExpression = struct {
    id: NodeId,
    statements: []Statement,
    // 块的最后一个表达式作为返回值。如果为空，则返回 unit
    result_expression: ?Expression,
    span: Span,
};

// --- 类型构造表达式 ---

pub const PointerTypeExpression = struct {
    id: NodeId,
    is_mutable: bool, // true: &mut T / *mut T
    is_volatile: bool, // true: *T (驱动开发用)
    child_type: Expression, // 指向的类型
    span: Span,
};

pub const SliceTypeExpression = struct {
    id: NodeId,
    child_type: Expression, // []T 中的 T
    span: Span,
};

pub const ArrayTypeExpression = struct {
    id: NodeId,
    size: Expression, // [N]T 中的 N (必须是编译期常量)
    child_type: Expression, // [N]T 中的 T
    span: Span,
};

/// 可选类型表达式 ?T
pub const OptionalTypeExpression = struct {
    id: NodeId,
    child_type: Expression,
    span: Span,
};

/// 函数类型表达式
/// 例如: fn(i32, i32) i32
pub const FunctionTypeExpression = struct {
    id: NodeId,
    parameters: []Expression, // 参数类型列表
    return_type: ?Expression, // 返回值类型 (null 表示 unit)
    is_variadic: bool, // 是否包含 ... (C FFI)
    span: Span,
};

pub const NeverTypeExpression = struct {
    id: NodeId,
    span: Span,
};

/// 模式 (Pattern)
/// 用于 `let`, `match`, 函数参数解构
pub const Pattern = union(enum) {
    Wildcard: WildcardPattern, // _
    Literal: Literal, // 1, "abc", true
    IdentifierBinding: IdentifierBindingPattern, // x, mut x
    StructDestructuring: StructDestructuringPattern, // Point { x, y }
    TupleDestructuring: TupleDestructuringPattern, // (a, b)
    EnumMatching: EnumMatchingPattern, // .Ok(v) 或 Result.Ok(v)
    Range: RangePattern, // 1..100

    pub fn span(self: Pattern) Span {
        return switch (self) {
            .Wildcard => |v| v.span,
            .Literal => |v| v.span,
            .IdentifierBinding => |v| v.span,
            .StructDestructuring => |v| v.span,
            .TupleDestructuring => |v| v.span,
            .EnumMatching => |v| v.span,
            .Range => |v| v.span,
        };
    }

    pub fn id(self: Pattern) NodeId {
        return switch (self) {
            .Wildcard => |v| v.id,
            .Literal => |v| v.id,
            .IdentifierBinding => |v| v.id,
            .StructDestructuring => |v| v.id,
            .TupleDestructuring => |v| v.id,
            .EnumMatching => |v| v.id,
            .Range => |v| v.id,
        };
    }
};

pub const IdentifierBindingPattern = struct {
    id: NodeId,
    name: SymbolId,
    is_mutable: bool, // let mut x = ...
    span: Span,
};

pub const StructDestructuringPattern = struct {
    id: NodeId,
    type_expression: ?Expression, // Point { ... }
    fields: []PatternStructField,
    ignore_remaining: bool,
    span: Span,
};

pub const PatternStructField = struct {
    id: NodeId,
    field_name: SymbolId,
    pattern: Pattern, // field: pattern
    span: Span,
};

pub const WildcardPattern = struct {
    id: NodeId,
    span: Span,
};

pub const TupleDestructuringPattern = struct {
    id: NodeId,
    elements: []Pattern,
    span: Span,
};

pub const EnumMatchingPattern = struct {
    id: NodeId,
    variant_name: SymbolId, // Ok
    type_context: ?Expression, // Result.Ok 中的 Result (可选，如果是 .Ok)
    payloads: []Pattern, // Ok(v) 中的 v
    span: Span,
};

// pattern 出现在enum中要求是字面量
pub const RangePattern = struct {
    id: NodeId,
    start: Literal,
    end: Literal,
    is_inclusive: bool,
    span: Span,
};

/// 语句 (Statement)
pub const Statement = union(enum) {
    Let: *LetStatement,
    // 声明作为语句
    Declaration: Declaration,

    // 表达式语句 (例如函数调用 `do_something();`)
    ExpressionStatement: Expression,

    For: *ForStatement,
    Break: *BreakStatement,
    Continue: *ContinueStatement,
    Return: *ReturnStatement,
    Defer: *DeferStatement,

    pub fn span(self: Statement) Span {
        return switch (self) {
            .Let => |v| v.span,
            .Declaration => |v| v.span(),
            .ExpressionStatement => |v| v.span(),
            .For => |v| v.span,
            .Break => |v| v.span,
            .Continue => |v| v.span,
            .Return => |v| v.span,
            .Defer => |v| v.span,
        };
    }
};

pub const LetStatement = struct {
    id: NodeId,
    pattern: Pattern,
    type_annotation: ?Expression, // : T
    value: Expression,
    span: Span,
};

pub const ConstStatement = struct {
    id: NodeId,
    name: SymbolId, // const 通常必须显式命名，不能解构太复杂
    type_annotation: ?Expression,
    value: Expression,
    span: Span,
};

pub const ForStatement = struct {
    id: NodeId,
    // 三段式: for init; condition; post
    initializer: ?*Statement, // var i = 0
    condition: ?Expression, // i < 10
    post_iteration: ?Expression, // i += 1
    body: *BlockExpression,
    span: Span,
};

pub const BreakStatement = struct {
    id: NodeId,
    span: Span,
};

pub const ContinueStatement = struct {
    id: NodeId,
    span: Span,
};

pub const ReturnStatement = struct {
    id: NodeId,
    value: ?Expression,
    span: Span,
};

pub const DeferStatement = struct {
    id: NodeId,
    target: Expression, // defer expression (通常是 block 或 call)
    span: Span,
};

/// 声明
/// 顶层结构的定义
pub const Declaration = union(enum) {
    Function: *FunctionDeclaration,
    Struct: *StructDeclaration,
    Enum: *EnumDeclaration,
    Union: *UnionDeclaration,
    Trait: *TraitDeclaration,
    Implementation: *ImplementationDeclaration, // impl
    Macro: *MacroDeclaration,
    Use: *UseDeclaration,
    ExternBlock: *ExternBlockDeclaration,
    // 类型别名: type A = B;
    TypeAlias: *TypeAliasDeclaration,
    // 全局变量: pub const PI = 3.14;
    GlobalVar: *GlobalVarDeclaration,

    pub fn span(self: Declaration) Span {
        return switch (self) {
            .Function => |v| v.span,
            .Struct => |v| v.span,
            .Enum => |v| v.span,
            .Union => |v| v.span,
            .Trait => |v| v.span,
            .Implementation => |v| v.span,
            .Macro => |v| v.span,
            .Use => |v| v.span,
            .ExternBlock => |v| v.span,
            .TypeAlias => |v| v.span,
            .GlobalVar => |v| v.span,
        };
    }

    pub fn id(self: Declaration) NodeId {
        return switch (self) {
            .Function => |v| v.id,
            .Struct => |v| v.id,
            .Enum => |v| v.id,
            .Union => |v| v.id,
            .Trait => |v| v.id,
            .Implementation => |v| v.id,
            .Macro => |v| v.id,
            .Use => |v| v.id,
            .ExternBlock => |v| v.id,
            .TypeAlias => |v| v.id,
            .GlobalVar => |v| v.id,
        };
    }
};

pub const FunctionDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter,
    is_extern: bool,
    parameters: []FunctionParameter,
    return_type: ?Expression, // unit 如果为 null
    body: ?*BlockExpression, // extern 函数没有 body
    span: Span,
};

pub const Visibility = enum {
    Private,
    Public,
};

pub const GenericParameter = struct {
    id: NodeId,
    name: SymbolId,
    constraints: []Expression,
    default_value: ?Expression,
    span: Span,
};

pub const FunctionParameter = struct {
    id: NodeId,
    name: SymbolId,
    type_expression: Expression,
    default_value: ?Expression, // a: i32 = 0
    // 标记是否是 Binding Cast 参数
    // true 表示语法是 `name: as Type`
    // false 表示语法是 `name: Type`
    is_binding_cast: bool,
    is_variadic: bool, // ... (C FFI)
    span: Span,
};

pub const StructDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter, // struct Point(T: Any)
    //  基类: struct Button : Widget
    // 如果没有继承，则为 null
    base_type: ?Expression,
    fields: []StructFieldDeclaration,
    // 静态成员 : 命名空间内容 (pub const A = 1, fn new(), impl...)
    // 这里可以包含 impl 块，也可以包含嵌套的 struct
    declarations: []Declaration,
    span: Span,
};

pub const StructFieldDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    type_expression: Expression,
    default_value: ?Expression,
    span: Span,
};

pub const EnumDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter,
    underlying_type: ?Expression, // enum Color: i32
    variants: []EnumVariant,
    span: Span,
};

pub const EnumVariant = struct {
    id: NodeId,
    name: SymbolId,
    // 枚举变体可以是：
    // 1. Unit (Quit)
    // 2. Value (Quit = 3)
    // 3. Struct-like (Move {x: i32})
    // 4. Tuple-like (Write(String))
    kind: union(enum) {
        None,
        Value: Expression,
        StructLike: []StructFieldDeclaration,
        TupleLike: []Expression, // 类型列表
    },
    span: Span,
};

pub const UnionDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter,
    variants: []UnionVariant,
    span: Span,
};

pub const UnionVariant = struct {
    id: NodeId,
    name: SymbolId,
    type_expression: Expression,
    span: Span,
};

pub const TraitDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter,
    super_traits: []Expression,
    methods: []FunctionDeclaration, // trait 里的函数通常只有签名
    span: Span,
};

pub const ImplementationDeclaration = struct {
    id: NodeId,
    // impl<T> 的泛型参数
    generics: []GenericParameter,

    // impl Point<f32>
    target_type: Expression,
    // impl Type: Trait
    trait_interface: ?Expression,
    // 允许: fn (methods), const, static, struct, union...
    // 限制: Parser 必须在解析 impl 内部时，禁止解析嵌套的 impl (if tag == .Implementation error)
    declarations: []Declaration,
    span: Span,
};

pub const UseDeclaration = struct {
    id: NodeId,
    visibility: Visibility,
    path: Expression, // std.debug.print (MemberAccess链)
    alias: ?SymbolId, // as P
    is_glob: bool, // use std.math.*
    // 如果是 `use std.{a, b}` 这种组合导入，Parser 可能会展开成多个 UseDecl
    span: Span,
};

pub const ExternBlockDeclaration = struct {
    id: NodeId,
    // extern { ... }
    declarations: []Declaration,
    span: Span,
};

// 类型别名定义
pub const TypeAliasDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    generics: []GenericParameter, // 支持 type Callback<T> = fn(T) void;
    target: Expression, // 右边的类型表达式
    span: Span,
};

// 全局变量定义
pub const GlobalVarDeclaration = struct {
    id: NodeId,
    kind: GlobalVarKind,
    visibility: Visibility,
    name: SymbolId,
    type_annotation: ?Expression,
    value: Expression,
    span: Span,
};

pub const GlobalVarKind = enum { Static, StaticMut, Const };

/// 宏定义
/// 宏片段说明符 (对应 $x:expr 中的 expr)
pub const MacroFragmentSpecifier = enum {
    Expression, // :expr
    Identifier, // :ident
    Type, // :type 或 :ty
    Statement, // :stmt
    Block, // :block
    Path, // :path
    Literal, // :literal (字符串、数字等)
    TokenTree, // :tt (任意 Token，最通用)
};

/// 宏匹配器的一个单元
pub const MacroMatcher = union(enum) {
    /// 字面量匹配: 比如 ( $x:expr, $y:expr ) 中的逗号
    Literal: Token,

    /// 参数捕获: $name:specifier
    Argument: struct {
        name: SymbolId,
        fragment: MacroFragmentSpecifier,
        span: Span,
    },

    /// 重复模式: $( ... ) sep op
    /// 例如: $( $x:expr ),*
    Repetition: struct {
        matchers: []MacroMatcher, // 括号内的子匹配器序列
        separator: ?Token, // 可选分隔符 (例如逗号)
        op: MacroRepetitionOp, //重复操作符 (*, +, ?)
        span: Span,
    },
};

/// 重复操作符
pub const MacroRepetitionOp = enum {
    ZeroOrMore, // *
    OneOrMore, // +
    ZeroOrOne, // ?
};

pub const MacroRule = struct {
    id: NodeId,
    matchers: []MacroMatcher, // 匹配模式序列
    body: []const Token,
    span: Span,
};

pub const MacroDeclaration = struct {
    id: NodeId,
    name: SymbolId,
    visibility: Visibility,
    rules: []MacroRule,
    span: Span,
};

/// 模块 (Module) - AST 的根节点
pub const Module = struct {
    declarations: []Declaration,
};
