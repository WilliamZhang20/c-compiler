use model::Token;
use regex_lite::Regex;
use std::collections::HashMap;

#[derive(PartialEq, Eq, Hash)]
enum TokenType {
    Identifier,
    Constant,
}

pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();

    let Ok(block_comment_regex) = Regex::new("\\/\\*[\\s\\S]*?\\*\\/") else {
        return Err("Failed to compile block comment regex".to_string());
    };
    let Ok(identifier_regex) = Regex::new("^[a-zA-Z]\\w*\\b") else {
        return Err("Failed to compile identifier regex".to_string());
    };
    let Ok(constant_regex) = Regex::new("^[0-9]+\\b") else {
        return Err("Failed to compile constant regex".to_string());
    };

    let mut regexes = HashMap::new();
    regexes.insert(TokenType::Identifier, identifier_regex);
    regexes.insert(TokenType::Constant, constant_regex);

    let mut input = input.to_string();
    while !input.is_empty() {
        input = input.trim_start().to_string();

        // if the string is empty after trimming then go to the next line
        if input.is_empty() {
            continue;
        }

        // remove C style block comments
        input = block_comment_regex.replace_all(&input, "").to_string();

        // handle C++ style comments
        if input.starts_with("//") {
            let (_, input_str) = input.split_once('\n').unwrap();
            input = input_str.to_string();
            continue;
        }

        // handle newlines
        if input.starts_with('\n') {
            let (_, input_str) = input.split_at(1);
            input = input_str.to_string();
            continue;
        }

    // handle braces, semicolons, and commas
    if input.starts_with([';', '(', ')', '{', '}', ',']) {
        let (token, new_input) = input.split_at(1);
        let token = match token {
            ";" => Token::Semicolon,
            "(" => Token::OpenParenthesis,
            ")" => Token::CloseParenthesis,
            "{" => Token::OpenBrace,
            "}" => Token::CloseBrace,
            "," => Token::Comma,
            _ => unreachable!("This should never happen"),
        };
        tokens.push(token);
        input = new_input.to_string();
        continue;
    }

    // handle operators
    if input.starts_with("==") {
        tokens.push(Token::EqualEqual);
        input = input[2..].to_string();
        continue;
    }
    if input.starts_with("!=") {
        tokens.push(Token::BangEqual);
        input = input[2..].to_string();
        continue;
    }
    if input.starts_with("<=") {
        tokens.push(Token::LessEqual);
        input = input[2..].to_string();
        continue;
    }
    if input.starts_with(">=") {
        tokens.push(Token::GreaterEqual);
        input = input[2..].to_string();
        continue;
    }
    if input.starts_with("&&") {
        tokens.push(Token::AndAnd);
        input = input[2..].to_string();
        continue;
    }
    if input.starts_with("||") {
        tokens.push(Token::OrOr);
        input = input[2..].to_string();
        continue;
    }

    if input.starts_with(['+', '-', '*', '/', '=', '<', '>', '!']) {
        let (op, new_input) = input.split_at(1);
        let token = match op {
            "+" => Token::Plus,
            "-" => Token::Minus,
            "*" => Token::Star,
            "/" => Token::Slash,
            "=" => Token::Equal,
            "<" => Token::Less,
            ">" => Token::Greater,
            "!" => Token::Bang,
            _ => unreachable!("This should never happen"),
        };
        tokens.push(token);
        input = new_input.to_string();
        continue;
    }

    // find longest match at start of input for any regex in Table 1-1
    let mut longest_capture = "".to_string();
    let mut token_type = None;

    for (tok_type, re) in &regexes {
        let Some(caps) = re.captures(&input) else { continue };
        if caps[0].len() > longest_capture.len() {
            longest_capture = caps[0].to_string();
            token_type = Some(tok_type);
        }
    }

    // if no match is found, raise an error
    let Some(token_type) = token_type else {
        return Err("Found invalid token".to_string());
    };

    // convert matching substring into a token
    let mut token = match token_type {
        TokenType::Identifier => Token::Identifier { value: longest_capture.trim().to_string() },
        TokenType::Constant => {
            let value = longest_capture.trim().parse::<i64>()
                .map_err(|_| format!("Failed to parse constant: {}", longest_capture))?;
            Token::Constant { value }
        }
    };

    // convert identifiers that match keywords into keyword tokens
    token = match token {
        Token::Identifier { value } => match value.as_str() {
            "int" => Token::Int,
            "void" => Token::Void,
            "return" => Token::Return,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "do" => Token::Do,
            _ => Token::Identifier { value },
        },
        other => other,
    };

    // add token to the `tokens` vector at the end
    tokens.push(token);

    // remove matching substring from start of input
    input = input[longest_capture.len()..].to_string();
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