mod keywords;
mod literals;
mod state_machine;
#[cfg(test)]
mod repro_bug;

use model::Token;
use state_machine::StateMachineLexer;

/// Main lexer entry point using efficient state machine
pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut lexer = StateMachineLexer::new(input);
    lexer.tokenize()
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