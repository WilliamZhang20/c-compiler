use model::Token;
use regex_lite::Regex;
use std::sync::OnceLock;

#[derive(PartialEq, Eq, Hash)]
enum TokenType {
    Identifier,
    Constant,
    FloatLiteral,
    StringLiteral,
    CharLiteral,
}

pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();

    static BLOCK_COMMENT_REGEX: OnceLock<Regex> = OnceLock::new();
    let block_comment_regex = BLOCK_COMMENT_REGEX.get_or_init(|| {
        Regex::new(r"/\*[\s\S]*?\*/").expect("Failed to compile block comment regex")
    });
    
    // Pre-process: remove block comments and line comments are handled in-loop or pre-processed
    let input_processed = block_comment_regex.replace_all(input, "");
    let mut current_input: &str = &input_processed;

    static IDENTIFIER_REGEX: OnceLock<Regex> = OnceLock::new();
    let identifier_regex = IDENTIFIER_REGEX.get_or_init(|| {
        Regex::new(r"^[a-zA-Z_]\w*\b").expect("Failed to compile identifier regex")
    });

    static CONSTANT_REGEX: OnceLock<Regex> = OnceLock::new();
    let constant_regex = CONSTANT_REGEX.get_or_init(|| {
        Regex::new(r"^(0x[0-9a-fA-F]+|[0-9]+)\b").expect("Failed to compile constant regex")
    });

    static STRING_REGEX: OnceLock<Regex> = OnceLock::new();
    let string_regex = STRING_REGEX.get_or_init(|| {
        Regex::new(r#"^"(([^"]|\\")*)""#).expect("Failed to compile string regex")
    });

    static CHAR_REGEX: OnceLock<Regex> = OnceLock::new();
    let char_regex = CHAR_REGEX.get_or_init(|| {
        Regex::new(r"^'(\\.|[^'])'").expect("Failed to compile char regex")
    });
    static FLOAT_REGEX: OnceLock<Regex> = OnceLock::new();
    let float_regex = FLOAT_REGEX.get_or_init(|| {
        // Match float literals: digits.digits, .digits, or digits with exponent
        // Avoid matching "1." alone (which could be array[1].member)
        Regex::new(r"^([0-9]+\.[0-9]+|[0-9]*\.[0-9]+|[0-9]+[eE][+-]?[0-9]+)[fF]?").expect("Failed to compile float regex")
    });
    while !current_input.is_empty() {
        current_input = current_input.trim_start();
        if current_input.is_empty() {
            break;
        }

        // handle C++ style comments
        if current_input.starts_with("//") {
            if let Some((_, rest)) = current_input.split_once('\n') {
                current_input = rest;
            } else {
                current_input = "";
            }
            continue;
        }

        // handle preprocessor line markers (e.g., # 1 "file.c")
        if current_input.starts_with('#') {
            // Check if it's a line marker or a single '#' token
            // Line markers usually follow the pattern # line "file" ...
            // For now, we'll skip any line starting with # if we are at the start of a line (or start of input)
            // In a more robust implementation, we'd only skip if it's a valid line marker.
            if let Some((_, rest)) = current_input.split_once('\n') {
                current_input = rest;
            } else {
                current_input = "";
            }
            continue;
        }

        // handle multicharacter symbols
        if current_input.starts_with("...") {
            tokens.push(Token::Ellipsis);
            current_input = &current_input[3..];
            continue;
        }
        if current_input.starts_with("==") {
            tokens.push(Token::EqualEqual);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("!=") {
            tokens.push(Token::BangEqual);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("<=") {
            tokens.push(Token::LessEqual);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with(">=") {
            tokens.push(Token::GreaterEqual);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("&&") {
            tokens.push(Token::AndAnd);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("||") {
            tokens.push(Token::OrOr);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("<<") {
            tokens.push(Token::LessLess);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with(">>") {
            tokens.push(Token::GreaterGreater);
            current_input = &current_input[2..];
            continue;
        }
        if current_input.starts_with("->") {
            tokens.push(Token::Arrow);
            current_input = &current_input[2..];
            continue;
        }

        // Check for float literals BEFORE single character symbols
        // to avoid matching '3.14' as '3', '.', '14'
        if let Some(caps) = float_regex.captures(current_input) {
            let float_text = caps.get(0).unwrap().as_str();
            let float_str = float_text.trim_end_matches(|c| c == 'f' || c == 'F');
            let value = float_str.parse::<f64>().map_err(|_| format!("Failed to parse float literal: {}", float_text))?;
            tokens.push(Token::FloatLiteral { value });
            current_input = &current_input[float_text.len()..];
            continue;
        }

        // handle single character symbols
        if let Some(c) = current_input.chars().next() {
            let token = match c {
                ';' => Some(Token::Semicolon),
                '(' => Some(Token::OpenParenthesis),
                ')' => Some(Token::CloseParenthesis),
                '{' => Some(Token::OpenBrace),
                '}' => Some(Token::CloseBrace),
                ',' => Some(Token::Comma),
                '[' => Some(Token::OpenBracket),
                ']' => Some(Token::CloseBracket),
                '#' => Some(Token::Hash),
                ':' => Some(Token::Colon),
                '.' => Some(Token::Dot),
                '&' => Some(Token::Ampersand),
                '~' => Some(Token::Tilde),
                '+' => Some(Token::Plus),
                '-' => Some(Token::Minus),
                '*' => Some(Token::Star),
                '/' => Some(Token::Slash),
                '%' => Some(Token::Percent),
                '=' => Some(Token::Equal),
                '<' => Some(Token::Less),
                '>' => Some(Token::Greater),
                '!' => Some(Token::Bang),
                '|' => Some(Token::Pipe),
                '^' => Some(Token::Caret),
                _ => None,
            };
            if let Some(t) = token {
                tokens.push(t);
                current_input = &current_input[c.len_utf8()..];
                continue;
            }
        }

        // handle regex matches
        let mut longest_capture = "";
        let mut tok_type = None;

        if let Some(caps) = identifier_regex.captures(current_input) {
            longest_capture = caps.get(0).unwrap().as_str();
            tok_type = Some(TokenType::Identifier);
        }
        if let Some(caps) = constant_regex.captures(current_input) {
            let text = caps.get(0).unwrap().as_str();
            if text.len() > longest_capture.len() {
                longest_capture = text;
                tok_type = Some(TokenType::Constant);
            }
        }
        if let Some(caps) = string_regex.captures(current_input) {
            let text = caps.get(0).unwrap().as_str();
            if text.len() > longest_capture.len() {
                longest_capture = text;
                tok_type = Some(TokenType::StringLiteral);
            }
        }
        if let Some(caps) = char_regex.captures(current_input) {
            let text = caps.get(0).unwrap().as_str();
            if text.len() > longest_capture.len() {
                longest_capture = text;
                tok_type = Some(TokenType::CharLiteral);
            }
        }
        // Note: FloatLiteral is handled earlier to avoid conflicting with '.' token

        let Some(tt) = tok_type else {
            return Err(format!("Found invalid token: '{}' at \"{}\"...", current_input.chars().next().unwrap(), &current_input[..current_input.len().min(20)]));
        };

        let token = match tt {
            TokenType::Identifier => {
                let value = longest_capture.to_string();
                match value.as_str() {
                    "int" => Token::Int,
                    "void" => Token::Void,
                    "return" => Token::Return,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "while" => Token::While,
                    "for" => Token::For,
                    "do" => Token::Do,
                    "break" => Token::Break,
                    "continue" => Token::Continue,
                    "switch" => Token::Switch,
                    "case" => Token::Case,
                    "default" => Token::Default,
                    "static" => Token::Static,
                    "extern" => Token::Extern,
                    "inline" => Token::Inline,
                    "const" => Token::Const,
                    "typedef" => Token::Typedef,
                    "struct" => Token::Struct,
                    "char" => Token::Char,
                    "enum" => Token::Enum,
                    "float" => Token::Float,
                    "double" => Token::Double,
                    "__attribute__" => Token::Attribute,
                    "__extension__" => Token::Extension,
                    "restrict" => Token::Restrict,
                    "sizeof" => Token::SizeOf,
                    _ => Token::Identifier { value },
                }
            }
            TokenType::Constant => {
                let value = if longest_capture.starts_with("0x") {
                    i64::from_str_radix(&longest_capture[2..], 16).map_err(|_| format!("Failed to parse hex constant: {}", longest_capture))?
                } else {
                    longest_capture.parse::<i64>().map_err(|_| format!("Failed to parse constant: {}", longest_capture))?
                };
                Token::Constant { value }
            }
            TokenType::CharLiteral => {
                // Parse character literal to integer value
                let content = &longest_capture[1..longest_capture.len()-1];
                let value = if content.starts_with('\\') {
                    // Escape sequence
                    match content.chars().nth(1) {
                        Some('n') => 10,  // newline
                        Some('t') => 9,   // tab
                        Some('r') => 13,  // carriage return
                        Some('0') => 0,   // null
                        Some('\\') => 92, // backslash
                        Some('\'') => 39, // single quote
                        Some('"') => 34,  // double quote
                        Some(c) if c.is_ascii_digit() => {
                            // Octal escape sequence like '\077'
                            let octal = content[1..].chars().take_while(|ch| ch.is_ascii_digit()).collect::<String>();
                            i64::from_str_radix(&octal, 8).unwrap_or(0)
                        }
                        _ => return Err(format!("Unknown escape sequence in character literal: {}", longest_capture)),
                    }
                } else {
                    // Regular character
                    content.chars().next().unwrap() as i64
                };
                Token::Constant { value }
            }
            TokenType::FloatLiteral => {
                // Parse float literal, removing optional 'f' or 'F' suffix
                let float_str = longest_capture.trim_end_matches(|c| c == 'f' || c == 'F');
                let value = float_str.parse::<f64>().map_err(|_| format!("Failed to parse float literal: {}", longest_capture))?;
                Token::FloatLiteral { value }
            }
            TokenType::StringLiteral => {
                let value = longest_capture[1..longest_capture.len()-1].to_string();
                Token::StringLiteral { value }
            }
        };

        tokens.push(token);
        current_input = &current_input[longest_capture.len()..];
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_identifier_and_constant() {
        let input = "foo 123";
        let tokens = lex(input).expect("lexing should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::Identifier { value: "foo".to_string() },
                Token::Constant { value: 123 },
            ]
        );
    }

    #[test]
    fn lex_keywords_and_operators() {
        let input = "int x = 1; if (x == 1) return;";
        let tokens = lex(input).expect("lexing should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier { value: "x".to_string() },
                Token::Equal,
                Token::Constant { value: 1 },
                Token::Semicolon,
                Token::If,
                Token::OpenParenthesis,
                Token::Identifier { value: "x".to_string() },
                Token::EqualEqual,
                Token::Constant { value: 1 },
                Token::CloseParenthesis,
                Token::Return,
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn lex_ignores_comments_and_whitespace() {
        let input = r#"
            // line comment
            int /* block comment */ x = 2;
        "#;
        let tokens = lex(input).expect("lexing should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::Int,
                Token::Identifier { value: "x".to_string() },
                Token::Equal,
                Token::Constant { value: 2 },
                Token::Semicolon,
            ]
        );
    }
}