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
    Static,
    Extern,
    Inline,
    Const,
    Typedef,
    Struct,
    Char,
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
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Bang,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Int,
    Void,
    Array(Box<Type>, usize),
    Pointer(Box<Type>),
    Char,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
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
        init: Option<Expr>,
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
        name: String,
        args: Vec<Expr>,
    },
    SizeOf(Type),
    SizeOfExpr(Box<Expr>),
    Cast(Type, Box<Expr>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    Assign,
}

#[derive(Debug, PartialEq, Clone)]
pub enum UnaryOp {
    Plus,
    Minus,
    LogicalNot,
    AddrOf,
    Deref,
}
