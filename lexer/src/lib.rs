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

        // handle braces and semicolons
        if input.starts_with([';', '(', ')', '{', '}']) {
            let (token, new_input) = input.split_at(1);
            let token = match token {
                ";" => Token::Semicolon,
                "(" => Token::OpenParenthesis,
                ")" => Token::CloseParenthesis,
                "{" => Token::OpenBrace,
                "}" => Token::CloseBrace,
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
        let token = match token_type {
            TokenType::Identifier => Token::Identifier { value: longest_capture.trim().to_string() },
            // TODO
            TokenType::Constant => Token::Constant { value: todo!("need to parse contstant as i64") /*longest_capture.trim()*/ }
        };

        // add token to the `tokens` vector at the end

        // remove matching substring from start of input
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
}