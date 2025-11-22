#[derive(PartialEq, Debug)]
pub enum Token {
    Identifier { value: String },
    Constant { value: i64 },
    OpenParenthesis,
    CloseParenthesis,
    OpenBrace,
    CloseBrace,
    Semicolon,
    // Keywords
    Int,
    Void,
    Return,
    If,
    Else,
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    EqualEqual,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Int,
    Void,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Function {
    pub return_type: Type,
    pub name: String,
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
    Block(Block),
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
}

#[derive(Debug, PartialEq, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    EqualEqual,
    Assign,
}

#[derive(Debug, PartialEq, Clone)]
pub enum UnaryOp {
    Plus,
    Minus,
}
