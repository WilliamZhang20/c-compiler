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
