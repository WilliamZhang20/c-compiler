pub enum Token {
    Identifier { value: String },
    Constant { value: i64 },
    OpenParenthesis,
    CloseParenthesis,
    OpenBrace,
    CloseBrace,
    Semicolon,
}
