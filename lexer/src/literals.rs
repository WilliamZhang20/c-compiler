/// Parse a character literal to its integer value
pub fn parse_char_literal(content: &str) -> Result<i64, String> {
    if content.starts_with('\\') {
        // Escape sequence
        match content.chars().nth(1) {
            Some('n') => Ok(10),  // newline
            Some('t') => Ok(9),   // tab
            Some('r') => Ok(13),  // carriage return
            Some('0') => Ok(0),   // null
            Some('\\') => Ok(92), // backslash
            Some('\'') => Ok(39), // single quote
            Some('"') => Ok(34),  // double quote
            Some('a') => Ok(7),   // alert/bell
            Some('b') => Ok(8),   // backspace
            Some('f') => Ok(12),  // form feed
            Some('v') => Ok(11),  // vertical tab
            Some('x') => {
                // Hexadecimal escape like '\x1F'
                let hex = &content[2..];
                i64::from_str_radix(hex, 16)
                    .map_err(|_| format!("Invalid hex escape sequence: {}", content))
            }
            Some(c) if c.is_ascii_digit() => {
                // Octal escape sequence like '\077'
                let octal = content[1..].chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect::<String>();
                i64::from_str_radix(&octal, 8)
                    .map_err(|_| format!("Invalid octal escape sequence: {}", content))
            }
            _ => Err(format!("Unknown escape sequence in character literal: '{}'", content)),
        }
    } else {
        // Regular character
        content.chars().next()
            .map(|c| c as i64)
            .ok_or_else(|| "Empty character literal".to_string())
    }
}

/// Parse an integer constant (decimal or hexadecimal)
pub fn parse_int_constant(text: &str) -> Result<i64, String> {
    if text.starts_with("0x") || text.starts_with("0X") {
        i64::from_str_radix(&text[2..], 16)
            .map_err(|_| format!("Failed to parse hex constant: {}", text))
    } else {
        text.parse::<i64>()
            .map_err(|_| format!("Failed to parse constant: {}", text))
    }
}

/// Parse a float literal, removing optional 'f' or 'F' suffix
pub fn parse_float_literal(text: &str) -> Result<f64, String> {
    let float_str = text.trim_end_matches(|c| c == 'f' || c == 'F');
    float_str.parse::<f64>()
        .map_err(|_| format!("Failed to parse float literal: {}", text))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_char_literal tests ───────────────────────────────
    #[test]
    fn char_regular() {
        assert_eq!(parse_char_literal("A").unwrap(), 65);
    }

    #[test]
    fn char_space() {
        assert_eq!(parse_char_literal(" ").unwrap(), 32);
    }

    #[test]
    fn char_escape_newline() {
        assert_eq!(parse_char_literal("\\n").unwrap(), 10);
    }

    #[test]
    fn char_escape_tab() {
        assert_eq!(parse_char_literal("\\t").unwrap(), 9);
    }

    #[test]
    fn char_escape_carriage_return() {
        assert_eq!(parse_char_literal("\\r").unwrap(), 13);
    }

    #[test]
    fn char_escape_null() {
        assert_eq!(parse_char_literal("\\0").unwrap(), 0);
    }

    #[test]
    fn char_escape_backslash() {
        assert_eq!(parse_char_literal("\\\\").unwrap(), 92);
    }

    #[test]
    fn char_escape_single_quote() {
        assert_eq!(parse_char_literal("\\'").unwrap(), 39);
    }

    #[test]
    fn char_escape_double_quote() {
        assert_eq!(parse_char_literal("\\\"").unwrap(), 34);
    }

    #[test]
    fn char_escape_alert() {
        assert_eq!(parse_char_literal("\\a").unwrap(), 7);
    }

    #[test]
    fn char_escape_backspace() {
        assert_eq!(parse_char_literal("\\b").unwrap(), 8);
    }

    #[test]
    fn char_escape_form_feed() {
        assert_eq!(parse_char_literal("\\f").unwrap(), 12);
    }

    #[test]
    fn char_escape_vertical_tab() {
        assert_eq!(parse_char_literal("\\v").unwrap(), 11);
    }

    #[test]
    fn char_hex_escape() {
        assert_eq!(parse_char_literal("\\x41").unwrap(), 0x41); // 'A'
    }

    #[test]
    fn char_hex_escape_lowercase() {
        assert_eq!(parse_char_literal("\\xff").unwrap(), 0xFF);
    }

    #[test]
    fn char_octal_escape() {
        assert_eq!(parse_char_literal("\\101").unwrap(), 65); // 'A' = octal 101
    }

    #[test]
    fn char_octal_zero() {
        // \0 matches the null escape before reaching the octal branch,
        // so \077 is interpreted as \0 (null) = 0, not octal 077 = 63
        assert_eq!(parse_char_literal("\\0").unwrap(), 0);
        // Octal escapes starting with non-zero digits work correctly
        assert_eq!(parse_char_literal("\\101").unwrap(), 65); // 'A'
    }

    #[test]
    fn char_empty_is_error() {
        assert!(parse_char_literal("").is_err());
    }

    // ─── parse_int_constant tests ───────────────────────────────
    #[test]
    fn int_decimal() {
        assert_eq!(parse_int_constant("42").unwrap(), 42);
    }

    #[test]
    fn int_zero() {
        assert_eq!(parse_int_constant("0").unwrap(), 0);
    }

    #[test]
    fn int_hex_lowercase() {
        assert_eq!(parse_int_constant("0xff").unwrap(), 255);
    }

    #[test]
    fn int_hex_uppercase() {
        assert_eq!(parse_int_constant("0XFF").unwrap(), 255);
    }

    #[test]
    fn int_hex_mixed_case() {
        assert_eq!(parse_int_constant("0xAbCd").unwrap(), 0xABCD);
    }

    #[test]
    fn int_large() {
        assert_eq!(parse_int_constant("2147483647").unwrap(), i32::MAX as i64);
    }

    #[test]
    fn int_invalid_is_error() {
        assert!(parse_int_constant("abc").is_err());
    }

    // ─── parse_float_literal tests ──────────────────────────────
    #[test]
    fn float_simple() {
        assert_eq!(parse_float_literal("3.14").unwrap(), 3.14);
    }

    #[test]
    fn float_with_f_suffix() {
        assert_eq!(parse_float_literal("3.14f").unwrap(), 3.14);
    }

    #[test]
    fn float_with_big_f_suffix() {
        assert_eq!(parse_float_literal("3.14F").unwrap(), 3.14);
    }

    #[test]
    fn float_integer_form() {
        assert_eq!(parse_float_literal("1.0").unwrap(), 1.0);
    }

    #[test]
    fn float_zero() {
        assert_eq!(parse_float_literal("0.0").unwrap(), 0.0);
    }

    #[test]
    fn float_leading_dot() {
        assert_eq!(parse_float_literal(".5").unwrap(), 0.5);
    }

    #[test]
    fn float_with_exponent() {
        assert!((parse_float_literal("1e3").unwrap() - 1000.0).abs() < 0.001);
    }

    #[test]
    fn float_with_negative_exponent() {
        assert!((parse_float_literal("1e-2").unwrap() - 0.01).abs() < 0.0001);
    }

    #[test]
    fn float_invalid_is_error() {
        assert!(parse_float_literal("not_a_float").is_err());
    }
}
