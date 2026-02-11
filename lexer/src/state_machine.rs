use model::Token;
use crate::keywords::keyword_or_identifier;
use crate::literals::{parse_char_literal, parse_int_constant, parse_float_literal};

pub struct StateMachineLexer<'a> {
    input: &'a [u8],
    pos: usize,
    token_start: usize,
    at_line_start: bool,
}

impl<'a> StateMachineLexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            token_start: 0,
            at_line_start: true,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while self.pos < self.input.len() {
            match self.lex_next_token()? {
                Some(token) => tokens.push(token),
                None => continue, // Whitespace or comment consumed
            }
        }

        Ok(tokens)
    }

    fn lex_next_token(&mut self) -> Result<Option<Token>, String> {
        self.skip_whitespace();
        
        if self.pos >= self.input.len() {
            return Ok(None);
        }

        self.token_start = self.pos;
        let ch = self.current_char();

        match ch {
            // Comments
            '/' if self.peek(1) == Some('/') => {
                self.skip_line_comment();
                Ok(None)
            }
            '/' if self.peek(1) == Some('*') => {
                self.skip_block_comment()?;
                self.at_line_start = false;
                Ok(None)
            }
            // Preprocessor directives - skip entire line
            '#' if self.is_start_of_line() => {
                self.skip_preprocessor_line();
                self.at_line_start = false;
                Ok(None)
            }
            // String literals
            '"' => {
                self.at_line_start = false;
                self.lex_string()
            }
            // Character literals
            '\'' => {
                self.at_line_start = false;
                self.lex_char()
            }
            // Numbers
            '0'..='9' => {
                self.at_line_start = false;
                self.lex_number()
            }
            '.' if self.peek(1).map_or(false, |c| c.is_ascii_digit()) => {
                self.at_line_start = false;
                self.lex_number()
            }
            // Identifiers and keywords
            'a'..='z' | 'A'..='Z' | '_' => {
                self.at_line_start = false;
                self.lex_identifier()
            }
            // Operators and punctuation
            _ => {
                self.at_line_start = false;
                self.lex_operator_or_punctuation()
            }
        }
    }

    fn current_char(&self) -> char {
        self.input[self.pos] as char
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).map(|&b| b as char)
    }

    fn current_slice(&self) -> &str {
        std::str::from_utf8(&self.input[self.token_start..self.pos])
            .expect("Invalid UTF-8 in source")
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.current_char() {
                ' ' | '\t' | '\r' => self.pos += 1,
                '\n' => {
                    self.pos += 1;
                    self.at_line_start = true;
                }
                _ => break,
            }
        }
    }

    fn is_start_of_line(&self) -> bool {
        self.at_line_start
    }

    fn skip_line_comment(&mut self) {
        while self.pos < self.input.len() && self.current_char() != '\n' {
            self.pos += 1;
        }
        if self.pos < self.input.len() {
            self.pos += 1; // Skip the newline
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), String> {
        self.pos += 2; // Skip the initial /*
        
        while self.pos < self.input.len() {
            if self.current_char() == '*' && self.peek(1) == Some('/') {
                self.pos += 2;
                return Ok(());
            }
            self.pos += 1;
        }
        
        Err("Unterminated block comment".to_string())
    }

    fn skip_preprocessor_line(&mut self) {
        while self.pos < self.input.len() && self.current_char() != '\n' {
            self.pos += 1;
        }
        if self.pos < self.input.len() {
            self.pos += 1; // Skip the newline
        }
    }

    fn lex_string(&mut self) -> Result<Option<Token>, String> {
        self.pos += 1; // Skip opening quote
        let mut value = String::new();
        
        while self.pos < self.input.len() {
            match self.current_char() {
                '"' => {
                    self.pos += 1;
                    return Ok(Some(Token::StringLiteral { value }));
                }
                '\\' => {
                    self.pos += 1;
                    if let Some(ch) = self.peek(0) {
                        match ch {
                            '"' => { self.pos += 1; value.push('"'); }
                            '\\' => { self.pos += 1; value.push('\\'); }
                            'n' => { self.pos += 1; value.push('\n'); }
                            't' => { self.pos += 1; value.push('\t'); }
                            'r' => { self.pos += 1; value.push('\r'); }
                            '0' => { self.pos += 1; value.push('\0'); }
                            'a' => { self.pos += 1; value.push('\x07'); }
                            'b' => { self.pos += 1; value.push('\x08'); }
                            'f' => { self.pos += 1; value.push('\x0C'); }
                            'v' => { self.pos += 1; value.push('\x0B'); }
                            'x' => {
                                // Hexadecimal escape \xHH
                                self.pos += 1;
                                let hex_start = self.pos;
                                while self.pos < self.input.len() && self.pos - hex_start < 2 {
                                    if self.current_char().is_ascii_hexdigit() {
                                        self.pos += 1;
                                    } else {
                                        break;
                                    }
                                }
                                if self.pos > hex_start {
                                    let hex_str = std::str::from_utf8(&self.input[hex_start..self.pos])
                                        .map_err(|_| "Invalid UTF-8 in hex escape")?;
                                    let code = u8::from_str_radix(hex_str, 16)
                                        .map_err(|_| format!("Invalid hex escape: \\x{}", hex_str))?;
                                    value.push(code as char);
                                }
                            }
                            '0'..='7' => {
                                // Octal escape \ooo
                                let octal_start = self.pos;
                                while self.pos < self.input.len() && self.pos - octal_start < 3 {
                                    if matches!(self.current_char(), '0'..='7') {
                                        self.pos += 1;
                                    } else {
                                        break;
                                    }
                                }
                                let octal_str = std::str::from_utf8(&self.input[octal_start..self.pos])
                                    .map_err(|_| "Invalid UTF-8 in octal escape")?;
                                let code = u8::from_str_radix(octal_str, 8)
                                    .map_err(|_| format!("Invalid octal escape: \\{}", octal_str))?;
                                value.push(code as char);
                            }
                            _ => {
                                self.pos += 1;
                                value.push(ch);
                            }
                        }
                    }
                }
                ch => {
                    self.pos += 1;
                    value.push(ch);
                }
            }
        }
        
        Err("Unterminated string literal".to_string())
    }

    fn lex_char(&mut self) -> Result<Option<Token>, String> {
        self.pos += 1; // Skip opening quote
        
        if self.pos >= self.input.len() {
            return Err("Unterminated character literal".to_string());
        }

        let content_start = self.pos;
        
        // Handle escape sequences
        if self.current_char() == '\\' {
            self.pos += 1;
            if self.pos < self.input.len() {
                let escape_char = self.current_char();
                self.pos += 1;
                
                // For octal sequences, consume additional digits (up to 3 total)
                if escape_char.is_ascii_digit() && escape_char < '8' {
                    while self.pos < self.input.len() 
                          && self.current_char().is_ascii_digit() 
                          && self.current_char() < '8'
                          && self.pos - content_start < 4 {
                        self.pos += 1;
                    }
                }
                // For hex sequences \\xHH, consume hex digits
                else if escape_char == 'x' {
                    while self.pos < self.input.len() 
                          && self.current_char().is_ascii_hexdigit()
                          && self.pos - content_start < 4 {
                        self.pos += 1;
                    }
                }
            }
        } else {
            self.pos += 1;
        }
        
        if self.pos >= self.input.len() || self.current_char() != '\'' {
            return Err("Unterminated character literal".to_string());
        }
        
        let content = std::str::from_utf8(&self.input[content_start..self.pos])
            .expect("Invalid UTF-8 in char literal");
        let value = parse_char_literal(content)?;
        
        self.pos += 1; // Skip closing quote
        Ok(Some(Token::Constant { value }))
    }

    fn lex_number(&mut self) -> Result<Option<Token>, String> {
        // Check for hexadecimal
        if self.current_char() == '0' && matches!(self.peek(1), Some('x') | Some('X')) {
            return self.lex_hex_number();
        }

        let start = self.pos;
        let mut has_dot = false;
        let mut has_exp = false;

        // Consume digits
        while self.pos < self.input.len() {
            match self.current_char() {
                '0'..='9' => self.pos += 1,
                '.' if !has_dot && !has_exp => {
                    // Make sure it's not followed by something like a member access
                    if let Some(next) = self.peek(1) {
                        if next.is_ascii_digit() || matches!(next, 'e' | 'E') {
                            has_dot = true;
                            self.pos += 1;
                        } else {
                            break;
                        }
                    } else {
                        has_dot = true;
                        self.pos += 1;
                    }
                }
                'e' | 'E' if !has_exp => {
                    has_exp = true;
                    has_dot = true; // Float values can have exponents
                    self.pos += 1;
                    // Handle optional +/- after exponent
                    if matches!(self.peek(0), Some('+') | Some('-')) {
                        self.pos += 1;
                    }
                }
                'f' | 'F' if has_dot => {
                    self.pos += 1;
                    break;
                }
                _ => break,
            }
        }

        let text = std::str::from_utf8(&self.input[start..self.pos])
            .expect("Invalid UTF-8 in number");

        if has_dot || has_exp {
            let value = parse_float_literal(text)?;
            Ok(Some(Token::FloatLiteral { value }))
        } else {
            let value = parse_int_constant(text)?;
            Ok(Some(Token::Constant { value }))
        }
    }

    fn lex_hex_number(&mut self) -> Result<Option<Token>, String> {
        self.pos += 2; // Skip 0x
        let start = self.pos;

        while self.pos < self.input.len() {
            match self.current_char() {
                '0'..='9' | 'a'..='f' | 'A'..='F' => self.pos += 1,
                _ => break,
            }
        }

        if self.pos == start {
            return Err("Invalid hexadecimal number: no digits after 0x".to_string());
        }

        let text = std::str::from_utf8(&self.input[self.token_start..self.pos])
            .expect("Invalid UTF-8 in hex number");
        let value = parse_int_constant(text)?;
        Ok(Some(Token::Constant { value }))
    }

    fn lex_identifier(&mut self) -> Result<Option<Token>, String> {
        while self.pos < self.input.len() {
            match self.current_char() {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => self.pos += 1,
                _ => break,
            }
        }

        let text = self.current_slice();
        Ok(Some(keyword_or_identifier(text)))
    }

    fn lex_operator_or_punctuation(&mut self) -> Result<Option<Token>, String> {
        let ch = self.current_char();
        let next = self.peek(1);

        // Three-character operators
        if ch == '.' && next == Some('.') && self.peek(2) == Some('.') {
            self.pos += 3;
            return Ok(Some(Token::Ellipsis));
        }

        // Two-character operators
        let two_char_token = match (ch, next) {
            ('=', Some('=')) => Some(Token::EqualEqual),
            ('!', Some('=')) => Some(Token::BangEqual),
            ('<', Some('=')) => Some(Token::LessEqual),
            ('>', Some('=')) => Some(Token::GreaterEqual),
            ('&', Some('&')) => Some(Token::AndAnd),
            ('|', Some('|')) => Some(Token::OrOr),
            ('<', Some('<')) => Some(Token::LessLess),
            ('>', Some('>')) => Some(Token::GreaterGreater),
            ('-', Some('>')) => Some(Token::Arrow),
            ('+', Some('+')) => Some(Token::PlusPlus),
            ('-', Some('-')) => Some(Token::MinusMinus),
            // Compound assignments
            ('+', Some('=')) => Some(Token::PlusEqual),
            ('-', Some('=')) => Some(Token::MinusEqual),
            ('*', Some('=')) => Some(Token::StarEqual),
            ('/', Some('=')) => Some(Token::SlashEqual),
            ('%', Some('=')) => Some(Token::PercentEqual),
            ('&', Some('=')) => Some(Token::AndEqual),
            ('|', Some('=')) => Some(Token::OrEqual),
            ('^', Some('=')) => Some(Token::XorEqual),
            _ => None,
        };

        if let Some(token) = two_char_token {
            self.pos += 2;
            
            // Special check for <<= and >>= (3 chars)
            if matches!(token, Token::LessLess | Token::GreaterGreater) {
                if let Some('=') = self.peek(0) {
                     self.pos += 1;
                     return Ok(Some(if matches!(token, Token::LessLess) { Token::LessLessEqual } else { Token::GreaterGreaterEqual }));
                }
            }
            
            return Ok(Some(token));
        }

        // Single-character operators and punctuation
        self.pos += 1;
        let token = match ch {
            ';' => Token::Semicolon,
            '(' => Token::OpenParenthesis,
            ')' => Token::CloseParenthesis,
            '{' => Token::OpenBrace,
            '}' => Token::CloseBrace,
            ',' => Token::Comma,
            '[' => Token::OpenBracket,
            ']' => Token::CloseBracket,
            '#' => Token::Hash,
            ':' => Token::Colon,
            '?' => Token::Question,
            '.' => Token::Dot,
            '&' => Token::Ampersand,
            '~' => Token::Tilde,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '=' => Token::Equal,
            '<' => Token::Less,
            '>' => Token::Greater,
            '!' => Token::Bang,
            '|' => Token::Pipe,
            '^' => Token::Caret,
            _ => return Err(format!("Unexpected character: '{}'", ch)),
        };

        Ok(Some(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_basic() {
        let input = "int x = 123;";
        let mut lexer = StateMachineLexer::new(input);
        let tokens = lexer.tokenize().expect("Should tokenize");
        
        assert_eq!(tokens.len(), 5);
        assert!(matches!(tokens[0], Token::Int));
        assert!(matches!(tokens[1], Token::Identifier { .. }));
        assert!(matches!(tokens[2], Token::Equal));
        assert!(matches!(tokens[3], Token::Constant { value: 123 }));
        assert!(matches!(tokens[4], Token::Semicolon));
    }

    #[test]
    fn test_state_machine_float() {
        let input = "float x = 3.14;";
        let mut lexer = StateMachineLexer::new(input);
        let tokens = lexer.tokenize().expect("Should tokenize");
        
        assert!(matches!(tokens[3], Token::FloatLiteral { .. }));
    }

    #[test]
    fn test_state_machine_hex() {
        let input = "int x = 0xFF;";
        let mut lexer = StateMachineLexer::new(input);
        let tokens = lexer.tokenize().expect("Should tokenize");
        
        assert_eq!(tokens[3], Token::Constant { value: 255 });
    }

    #[test]
    fn test_state_machine_comments() {
        let input = "int /* comment */ x; // line comment\nint y;";
        let mut lexer = StateMachineLexer::new(input);
        let tokens = lexer.tokenize().expect("Should tokenize");
        
        // Should have: int x ; int y ;
        assert_eq!(tokens.len(), 6);
    }
}
