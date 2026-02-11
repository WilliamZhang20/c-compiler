use crate::lex;
use model::Token;

#[test]
fn test_float_starting_with_dot() {
    let input = ".123";
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens.len(), 1);
    match &tokens[0] {
         Token::FloatLiteral { value } => assert_eq!(*value, 0.123),
         _ => panic!("Expected FloatLiteral, got {:?}", tokens[0]),
    }
}

#[test]
fn test_compound_assignment() {
    let input = "+=";
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0], Token::PlusEqual);
}

#[test]
fn test_escape_sequences() {
    // Test various escape sequences in strings
    let input = r#""\a\b\f\v\0\x41\101""#;
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens.len(), 1);
    match &tokens[0] {
        Token::StringLiteral { value } => {
            assert_eq!(value.chars().nth(0), Some('\x07')); // \a
            assert_eq!(value.chars().nth(1), Some('\x08')); // \b
            assert_eq!(value.chars().nth(2), Some('\x0C')); // \f
            assert_eq!(value.chars().nth(3), Some('\x0B')); // \v
            assert_eq!(value.chars().nth(4), Some('\0'));   // \0
            assert_eq!(value.chars().nth(5), Some('A'));    // \x41
            assert_eq!(value.chars().nth(6), Some('A'));    // \101 (octal)
        }
        _ => panic!("Expected StringLiteral, got {:?}", tokens[0]),
    }
}

#[test]
fn test_char_escape_sequences() {
    // Test \x hex escape
    let input = "'\\x41'";
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0], Token::Constant { value: 0x41 }); // 'A'
    
    // Test \a alert
    let input = "'\\a'";
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens[0], Token::Constant { value: 7 });
    
    // Test \b backspace
    let input = "'\\b'";
    let tokens = lex(input).expect("lexing should succeed");
    assert_eq!(tokens[0], Token::Constant { value: 8 });
}
