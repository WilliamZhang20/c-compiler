#[derive(PartialEq, Debug, Clone)]
pub enum Token {
    Identifier { value: String },
    Constant { value: i64 },
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
    Static,
    Extern,
    Inline,
    Const,
    Typedef,
    Struct,
    Char,
    Enum,
    Switch,
    Case,
    Default,
    Hash, // #
    Ellipsis, // ...
    Colon, // :
    Dot, // .
    Ampersand, // &
    Tilde, // ~
    // Internal/Compiler Keywords often found in headers
    Attribute, // __attribute__
    Extension, // __extension__
    Restrict, // restrict
    SizeOf, // sizeof
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
}

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Int,
    Void,
    Array(Box<Type>, usize),
    Pointer(Box<Type>),
    Char,
    Struct(String),
    Typedef(String),
    FunctionPointer {
        return_type: Box<Type>,
        param_types: Vec<Type>,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
    pub globals: Vec<GlobalVar>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(Type, String)>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EnumDef {
    pub name: String,
    pub constants: Vec<(String, i64)>, // name => value
}

#[derive(Debug, PartialEq, Clone)]
pub struct GlobalVar {
    pub r#type: Type,
    pub name: String,
    pub init: Option<Expr>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Function {
    pub return_type: Type,
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub body: Block,
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
    Variable(String),
    Constant(i64),
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
    Cast(Type, Box<Expr>),
    Member {
        expr: Box<Expr>,
        member: String,
    },
    PtrMember {
        expr: Box<Expr>,
        member: String,
    },
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
