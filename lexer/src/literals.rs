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
