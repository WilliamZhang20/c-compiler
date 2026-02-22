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
    use model::IntegerSuffix;

    #[test]
    fn lex_simple_identifier_and_constant() {
        let input = "foo 123";
        let tokens = lex(input).expect("lexing should succeed");
        assert_eq!(
            tokens,
            vec![
                Token::Identifier { value: "foo".to_string() },
                Token::Constant { value: 123, suffix: IntegerSuffix::None },
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
                Token::Constant { value: 1, suffix: IntegerSuffix::None },
                Token::Semicolon,
                Token::If,
                Token::OpenParenthesis,
                Token::Identifier { value: "x".to_string() },
                Token::EqualEqual,
                Token::Constant { value: 1, suffix: IntegerSuffix::None },
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
                Token::Constant { value: 2, suffix: IntegerSuffix::None },
                Token::Semicolon,
            ]
        );
    }

    #[test]
    #[ignore] // Debug test for specific file - skip by default
    fn debug_tokens() {
        let src = std::fs::read_to_string("../hello_world.i").unwrap();
        let tokens = crate::lex(&src).unwrap();
        let pos: usize = 921;
        for i in (pos.saturating_sub(10))..(pos + 10).min(tokens.len()) {
            println!("{}: {:?}", i, tokens[i]);
        }
    }

    // ─── String literal tests ───────────────────────────────────
    #[test]
    fn lex_string_literal() {
        let tokens = lex(r#""hello world""#).unwrap();
        assert_eq!(tokens, vec![Token::StringLiteral { value: "hello world".to_string() }]);
    }

    #[test]
    fn lex_empty_string() {
        let tokens = lex(r#""""#).unwrap();
        assert_eq!(tokens, vec![Token::StringLiteral { value: "".to_string() }]);
    }

    // ─── Character literal tests ────────────────────────────────
    #[test]
    fn lex_char_literal() {
        let tokens = lex("'A'").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 65, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_char_newline_escape() {
        let tokens = lex(r"'\n'").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 10, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_multichar_constant() {
        // Multi-character constant 'AB' should pack big-endian: 'A'<<8 | 'B'
        let tokens = lex("'AB'").unwrap();
        let expected = (b'A' as i64) << 8 | (b'B' as i64);
        assert_eq!(tokens, vec![Token::Constant { value: expected, suffix: IntegerSuffix::None }]);
    }

    // ─── Numeric literal tests ──────────────────────────────────
    #[test]
    fn lex_hex_constant() {
        let tokens = lex("0x1F").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 31, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_hex_uppercase() {
        let tokens = lex("0XFF").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 255, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_zero() {
        let tokens = lex("0").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 0, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_float_with_exponent() {
        let tokens = lex("1e3").unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0], Token::FloatLiteral { value } if (value - 1000.0).abs() < 0.001));
    }

    #[test]
    fn lex_float_with_f_suffix() {
        let tokens = lex("3.14f").unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0], Token::FloatLiteral { value } if (value - 3.14).abs() < 0.001));
    }

    #[test]
    fn lex_integer_suffix_u() {
        // Integer with U suffix should still produce a Constant
        let tokens = lex("42U").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 42, suffix: IntegerSuffix::U }]);
    }

    #[test]
    fn lex_integer_suffix_ul() {
        let tokens = lex("100UL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 100, suffix: IntegerSuffix::UL }]);
    }

    #[test]
    fn lex_integer_suffix_ull() {
        let tokens = lex("999ULL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 999, suffix: IntegerSuffix::ULL }]);
    }

    #[test]
    fn lex_integer_suffix_ll() {
        let tokens = lex("123LL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 123, suffix: IntegerSuffix::LL }]);
    }

    #[test]
    fn lex_hex_with_suffix() {
        let tokens = lex("0xFFUL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 255, suffix: IntegerSuffix::UL }]);
    }

    // ─── Octal literal tests ────────────────────────────────────
    #[test]
    fn lex_octal_simple() {
        let tokens = lex("0777").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 0o777, suffix: IntegerSuffix::None }]); // 511
    }

    #[test]
    fn lex_octal_permissions() {
        let tokens = lex("0644").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 0o644, suffix: IntegerSuffix::None }]); // 420
    }

    #[test]
    fn lex_octal_small() {
        let tokens = lex("01").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 1, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_octal_with_suffix() {
        let tokens = lex("0777UL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 0o777, suffix: IntegerSuffix::UL }]);
    }

    #[test]
    fn lex_zero_still_works() {
        // Plain 0 should still be 0, not octal
        let tokens = lex("0").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 0, suffix: IntegerSuffix::None }]);
    }

    // ─── Binary literal tests ───────────────────────────────────
    #[test]
    fn lex_binary_simple() {
        let tokens = lex("0b1010").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 10, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_binary_uppercase() {
        let tokens = lex("0B11111111").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 255, suffix: IntegerSuffix::None }]);
    }

    #[test]
    fn lex_binary_with_suffix() {
        let tokens = lex("0b1010UL").unwrap();
        assert_eq!(tokens, vec![Token::Constant { value: 10, suffix: IntegerSuffix::UL }]);
    }

    // ─── Operator tests ─────────────────────────────────────────
    #[test]
    fn lex_all_compound_assignments() {
        let input = "+= -= *= /= %= &= |= ^= <<= >>=";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens, vec![
            Token::PlusEqual, Token::MinusEqual, Token::StarEqual,
            Token::SlashEqual, Token::PercentEqual, Token::AndEqual,
            Token::OrEqual, Token::XorEqual, Token::LessLessEqual,
            Token::GreaterGreaterEqual,
        ]);
    }

    #[test]
    fn lex_increment_decrement() {
        let tokens = lex("++ --").unwrap();
        assert_eq!(tokens, vec![Token::PlusPlus, Token::MinusMinus]);
    }

    #[test]
    fn lex_arrow_operator() {
        let tokens = lex("->").unwrap();
        assert_eq!(tokens, vec![Token::Arrow]);
    }

    #[test]
    fn lex_ellipsis() {
        let tokens = lex("...").unwrap();
        assert_eq!(tokens, vec![Token::Ellipsis]);
    }

    #[test]
    fn lex_shift_operators() {
        let tokens = lex("<< >>").unwrap();
        assert_eq!(tokens, vec![Token::LessLess, Token::GreaterGreater]);
    }

    #[test]
    fn lex_logical_operators() {
        let tokens = lex("&& || !").unwrap();
        assert_eq!(tokens, vec![Token::AndAnd, Token::OrOr, Token::Bang]);
    }

    #[test]
    fn lex_bitwise_operators() {
        let tokens = lex("& | ^ ~").unwrap();
        assert_eq!(tokens, vec![Token::Ampersand, Token::Pipe, Token::Caret, Token::Tilde]);
    }

    // ─── Keyword tests ──────────────────────────────────────────
    #[test]
    fn lex_c99_keywords() {
        let tokens = lex("_Bool _Generic _Alignof _Static_assert restrict").unwrap();
        assert_eq!(tokens, vec![
            Token::Bool, Token::Generic, Token::AlignOf,
            Token::StaticAssert, Token::Restrict,
        ]);
    }

    #[test]
    fn lex_gcc_extensions() {
        let tokens = lex("__attribute__ __extension__ __typeof__ __alignof__").unwrap();
        assert_eq!(tokens, vec![
            Token::Attribute, Token::Extension, Token::Typeof, Token::AlignOf,
        ]);
    }

    #[test]
    fn lex_storage_class_keywords() {
        let tokens = lex("static extern inline register").unwrap();
        assert_eq!(tokens, vec![
            Token::Static, Token::Extern, Token::Inline, Token::Register,
        ]);
    }

    #[test]
    fn lex_type_keywords() {
        let tokens = lex("int char void float double short long unsigned signed struct union enum").unwrap();
        assert_eq!(tokens, vec![
            Token::Int, Token::Char, Token::Void, Token::Float,
            Token::Double, Token::Short, Token::Long, Token::Unsigned,
            Token::Signed, Token::Struct, Token::Union, Token::Enum,
        ]);
    }

    #[test]
    fn lex_control_flow_keywords() {
        let tokens = lex("if else while for do switch case default break continue goto return").unwrap();
        assert_eq!(tokens, vec![
            Token::If, Token::Else, Token::While, Token::For,
            Token::Do, Token::Switch, Token::Case, Token::Default,
            Token::Break, Token::Continue, Token::Goto, Token::Return,
        ]);
    }

    // ─── Edge case tests ────────────────────────────────────────
    #[test]
    fn lex_empty_input() {
        let tokens = lex("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn lex_whitespace_only() {
        let tokens = lex("   \t\n  \r\n  ").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn lex_adjacent_tokens_no_space() {
        let tokens = lex("(x+1)").unwrap();
        assert_eq!(tokens, vec![
            Token::OpenParenthesis,
            Token::Identifier { value: "x".to_string() },
            Token::Plus,
            Token::Constant { value: 1, suffix: IntegerSuffix::None },
            Token::CloseParenthesis,
        ]);
    }

    #[test]
    fn lex_preprocessor_line_skipped() {
        let input = "# 1 \"test.c\"\nint x;";
        let tokens = lex(input).unwrap();
        // Preprocessor line should be skipped, only int x ; remain
        assert_eq!(tokens, vec![
            Token::Int,
            Token::Identifier { value: "x".to_string() },
            Token::Semicolon,
        ]);
    }

    #[test]
    fn lex_complex_expression() {
        let tokens = lex("a->b.c[0]").unwrap();
        assert_eq!(tokens, vec![
            Token::Identifier { value: "a".to_string() },
            Token::Arrow,
            Token::Identifier { value: "b".to_string() },
            Token::Dot,
            Token::Identifier { value: "c".to_string() },
            Token::OpenBracket,
            Token::Constant { value: 0, suffix: IntegerSuffix::None },
            Token::CloseBracket,
        ]);
    }
}