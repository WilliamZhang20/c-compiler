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
