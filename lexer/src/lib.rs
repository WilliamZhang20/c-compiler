use model::Token;
use regex_lite::Regex;

#[derive(PartialEq, Eq, Hash)]
enum TokenType {
    Identifier,
    Constant,
    StringLiteral,
}

pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();

    let Ok(block_comment_regex) = Regex::new("\\/\\*[\\s\\S]*?\\*\\/") else {
        return Err("Failed to compile block comment regex".to_string());
    };
    
    // Pre-process: remove block comments and line comments are handled in-loop or pre-processed
    let input_processed = block_comment_regex.replace_all(input, "");
    let mut current_input: &str = &input_processed;

    let Ok(identifier_regex) = Regex::new("^[a-zA-Z_]\\w*\\b") else {
        return Err("Failed to compile identifier regex".to_string());
    };
    let Ok(constant_regex) = Regex::new("^(0x[0-9a-fA-F]+|[0-9]+)\\b") else {
        return Err("Failed to compile constant regex".to_string());
    };
    let Ok(string_regex) = Regex::new("^\"(([^\"]|\\\\\")*)\"") else {
        return Err("Failed to compile string regex".to_string());
    };

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
        if current_input.starts_with("->") {
            // Treat as individual tokens for now? -> is usually for struct pointers.
            // But we don't support it yet, so let's just skip it as two symbols or a token.
            // I'll add a token later if needed. For now just consume.
            current_input = &current_input[2..];
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
                '=' => Some(Token::Equal),
                '<' => Some(Token::Less),
                '>' => Some(Token::Greater),
                '!' => Some(Token::Bang),
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
                    "static" => Token::Static,
                    "extern" => Token::Extern,
                    "inline" => Token::Inline,
                    "const" => Token::Const,
                    "typedef" => Token::Typedef,
                    "struct" => Token::Struct,
                    "char" => Token::Char,
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