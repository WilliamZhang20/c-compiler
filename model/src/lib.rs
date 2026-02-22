// Target platform configuration
mod target;
pub use target::{Platform, CallingConvention, TargetConfig};

/// Suffix on an integer constant, controlling its type.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum IntegerSuffix {
    /// No suffix — the type is `int` (or wider if the value is too large).
    #[default]
    None,
    /// `U` — unsigned int (or wider unsigned if value too large).
    U,
    /// `L` — long.
    L,
    /// `UL` / `LU` — unsigned long.
    UL,
    /// `LL` — long long.
    LL,
    /// `ULL` / `LLU` — unsigned long long.
    ULL,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Token {
    Identifier { value: String },
    Constant { value: i64, suffix: IntegerSuffix },
    FloatLiteral { value: f64 },
    StringLiteral { value: String },
    OpenParenthesis,
    CloseParenthesis,
    OpenBrace,
    CloseBrace,
    Semicolon,
    Comma,
    OpenBracket,
    CloseBracket,
    // Keywords
    Int,
    Void,
    Return,
    If,
    Else,
    While,
    For,
    Do,
    Break,
    Continue,
    Goto,
    Static,
    Extern,
    Inline,
    Asm,
    Const,
    Volatile,
    Typedef,
    Struct,
    Char,
    Enum,
    Float,
    Double,
    Switch,
    Case,
    Default,
    Unsigned,
    Signed,
    Long,
    Short,
    Union,
    Hash, // #
    Ellipsis, // ...
    Colon, // :
    Question, // ?
    Dot, // .
    Ampersand, // &
    Tilde, // ~
    // Internal/Compiler Keywords often found in headers
    Attribute, // __attribute__
    Extension, // __extension__
    Restrict, // restrict
    SizeOf, // sizeof
    Typeof, // typeof / __typeof__
    StaticAssert, // _Static_assert
    Bool, // _Bool
    AlignOf, // _Alignof / __alignof__
    Register, // register
    Generic, // _Generic
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LessLess,
    GreaterGreater,
    AndAnd,
    OrOr,
    Bang,
    Pipe,
    Caret,
    Arrow, // ->
    PlusPlus,   // ++
    MinusMinus, // --
    
    // Compound Assignment
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    AndEqual,
    OrEqual,
    XorEqual,
    LessLessEqual,
    GreaterGreaterEqual,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct TypeQualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Attribute {
    Packed,
    Aligned(usize),
    Section(String),
    NoReturn,
    AlwaysInline,
    Weak,
    Unused,
    Constructor,
    Destructor,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Int,
    UnsignedInt,
    Char,
    UnsignedChar,
    Short,
    UnsignedShort,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    Void,
    Float,
    Double,
    Array(Box<Type>, usize),
    Pointer(Box<Type>),
    Struct(String),
    Union(String),
    Typedef(String),
    FunctionPointer {
        return_type: Box<Type>,
        param_types: Vec<Type>,
    },
    Bool,
    /// `typeof(expr)` — resolved to the concrete type of the expression
    /// during IR lowering.
    TypeofExpr(Box<Expr>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
    pub globals: Vec<GlobalVar>,
    pub structs: Vec<StructDef>,
    pub unions: Vec<UnionDef>,
    pub enums: Vec<EnumDef>,
    pub prototypes: Vec<FunctionPrototype>,
    pub forward_structs: Vec<String>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FunctionPrototype {
    pub return_type: Type,
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub is_variadic: bool,
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructField {
    pub field_type: Type,
    pub name: String,
    pub bit_width: Option<usize>, // Some(n) for bit fields
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct UnionDef {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EnumDef {
    pub name: String,
    pub constants: Vec<(String, i64)>, // name => value
}

#[derive(Debug, PartialEq, Clone)]
pub struct GlobalVar {
    pub r#type: Type,
    pub qualifiers: TypeQualifiers,
    pub name: String,
    pub init: Option<Expr>,
    pub attributes: Vec<Attribute>,
    pub is_extern: bool,
    pub is_static: bool,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Function {
    pub return_type: Type,
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub body: Block,
    pub is_inline: bool,
    pub is_static: bool,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Stmt {
    Return(Option<Expr>),
    Expr(Expr),
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        body: Box<Stmt>,
        cond: Expr,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        post: Option<Expr>,
        body: Box<Stmt>,
    },
    Block(Block),
    Declaration {
        r#type: Type,
        qualifiers: TypeQualifiers,
        name: String,
        init: Option<Expr>,
    },
    Break,
    Continue,
    Switch {
        cond: Expr,
        body: Box<Stmt>,
    },
    Case(Expr),
    Default,
    Goto(String),  // label name
    Label(String), // label name
    /// A comma-separated multi-variable declaration lowered as flat siblings,
    /// sharing the scope of the enclosing block (no new scope created).
    MultiDecl(Vec<Stmt>),
    InlineAsm {
        template: String,     // assembly template
        outputs: Vec<AsmOperand>,
        inputs: Vec<AsmOperand>,
        clobbers: Vec<String>,
        is_volatile: bool,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct AsmOperand {
    pub constraint: String,  // "=r", "r", "m", etc.
    pub expr: Expr,          // variable or expression
}

#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    PostfixIncrement(Box<Expr>),
    PostfixDecrement(Box<Expr>),
    PrefixIncrement(Box<Expr>),
    PrefixDecrement(Box<Expr>),
    Variable(String),
    Constant(i64),
    FloatConstant(f64),
    StringLiteral(String),
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        func: Box<Expr>,  // Can be Variable(name) for direct calls or any expr for function pointers
        args: Vec<Expr>,
    },
    SizeOf(Type),
    SizeOfExpr(Box<Expr>),
    AlignOf(Type),
    Cast(Type, Box<Expr>),
    Member {
        expr: Box<Expr>,
        member: String,
    },
    PtrMember {
        expr: Box<Expr>,
        member: String,
    },
    Conditional {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    /// Comma operator: `(a, b, c)` — evaluates each sub-expression
    /// left-to-right, discarding all results except the last one.
    Comma(Vec<Expr>),
    /// Compound literal: `(type){init_list}` — creates a temporary with
    /// the given type and initializer.
    CompoundLiteral {
        r#type: Type,
        init: Vec<InitItem>,
    },
    /// GNU statement expression: `({ stmt; stmt; expr; })` — evaluates
    /// a block of statements and returns the value of the last expression.
    StmtExpr(Vec<Stmt>),
    /// Brace-enclosed initializer list: `{1, 2, 3}` or `{.x = 1, [0] = 2}`
    InitList(Vec<InitItem>),
    /// __builtin_offsetof(type, member) — compile-time offset of field in struct
    BuiltinOffsetof {
        r#type: Type,
        member: String,
    },
    /// _Generic(expr, type1: expr1, type2: expr2, ..., default: exprN)
    /// C11 generic selection — resolved at compile time based on type of controlling expr.
    Generic {
        controlling: Box<Expr>,
        associations: Vec<(Option<Type>, Expr)>,  // None = default
    },
}

/// A single item inside a brace-enclosed initializer list.
#[derive(Debug, PartialEq, Clone)]
pub struct InitItem {
    /// `None` for positional, `Some(...)` for designated (`.field` or `[index]`).
    pub designator: Option<Designator>,
    /// The value expression (may itself be an `InitList` for nested structs/arrays).
    pub value: Expr,
}

/// Designator in a designated initializer.
#[derive(Debug, PartialEq, Clone)]
pub enum Designator {
    /// `.field_name`
    Field(String),
    /// `[constant_index]`
    Index(i64),
}

#[derive(Debug, PartialEq, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    Assign,
    
    // Compound Assignment
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    BitwiseAndAssign,
    BitwiseOrAssign,
    BitwiseXorAssign,
    ShiftLeftAssign,
    ShiftRightAssign,
}

#[derive(Debug, PartialEq, Clone)]
pub enum UnaryOp {
    Plus,
    Minus,
    LogicalNot,
    BitwiseNot,
    AddrOf,
    Deref,
}
