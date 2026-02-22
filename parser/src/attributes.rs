use model::{Attribute, Token};
use crate::parser::Parser;

pub(crate) trait AttributeParser {
    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, String>;
}

impl<'a> AttributeParser for Parser<'a> {
    /// Parse __attribute__((...)) syntax and return a list of attributes
    fn parse_attributes(&mut self) -> Result<Vec<Attribute>, String> {
        let mut attributes = Vec::new();

        while self.match_token(|t| matches!(t, Token::Attribute | Token::Extension)) {
            // Expect (( after __attribute__
            if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                continue; // Just skip if no parentheses
            }
            if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                // Malformed attribute, skip to semicolon or close paren
                while !self.check(|t| matches!(t, Token::Semicolon | Token::CloseParenthesis)) && !self.is_at_end() {
                    self.advance();
                }
                continue;
            }

            // Parse attributes inside, comma-separated
            loop {
                if self.check(|t| matches!(t, Token::CloseParenthesis)) {
                    break;
                }

                // Parse individual attribute
                match self.peek() {
                    Some(Token::Identifier { value }) if value == "packed" => {
                        self.advance();
                        attributes.push(Attribute::Packed);
                    }
                    Some(Token::Identifier { value }) if value == "aligned" => {
                        self.advance();

                        // Parse aligned(N)
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            match self.advance() {
                                Some(Token::Constant { value, .. }) => {
                                    attributes.push(Attribute::Aligned(*value as usize));
                                }
                                other => {
                                    return Err(format!(
                                        "expected alignment constant, found {:?}",
                                        other
                                    ));
                                }
                            }
                            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        }
                    }
                    Some(Token::Identifier { value }) if value == "section" => {
                        self.advance();

                        // Parse section("name")
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            match self.advance() {
                                Some(Token::StringLiteral { value }) => {
                                    attributes.push(Attribute::Section(value.clone()));
                                }
                                other => {
                                    return Err(format!(
                                        "expected section name string, found {:?}",
                                        other
                                    ));
                                }
                            }
                            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        }
                    }
                    Some(Token::Identifier { value }) if value == "noreturn" => {
                        self.advance();
                        attributes.push(Attribute::NoReturn);
                    }
                    Some(Token::Identifier { value }) if value == "always_inline" => {
                        self.advance();
                        attributes.push(Attribute::AlwaysInline);
                    }
                    Some(Token::Identifier { value }) if value == "weak" || value == "__weak__" => {
                        self.advance();
                        attributes.push(Attribute::Weak);
                    }
                    Some(Token::Identifier { value }) if value == "unused" || value == "__unused__" => {
                        self.advance();
                        attributes.push(Attribute::Unused);
                    }
                    Some(Token::Identifier { value }) if value == "used" || value == "__used__" => {
                        self.advance();
                        // 'used' just means "don't GC this symbol" — we keep everything anyway
                        attributes.push(Attribute::Unused); // treat similarly
                    }
                    Some(Token::Identifier { value }) if value == "constructor" || value == "__constructor__" => {
                        self.advance();
                        // Skip optional priority: constructor(priority)
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            while !self.check(|t| matches!(t, Token::CloseParenthesis)) && !self.is_at_end() {
                                self.advance();
                            }
                            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        }
                        attributes.push(Attribute::Constructor);
                    }
                    Some(Token::Identifier { value }) if value == "destructor" || value == "__destructor__" => {
                        self.advance();
                        // Skip optional priority: destructor(priority)
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            while !self.check(|t| matches!(t, Token::CloseParenthesis)) && !self.is_at_end() {
                                self.advance();
                            }
                            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        }
                        attributes.push(Attribute::Destructor);
                    }
                    _ => {
                        // Skip unknown attributes
                        self.advance();
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            let _ = self.skip_parentheses();
                        }
                    }
                }

                if !self.match_token(|t| matches!(t, Token::Comma)) {
                    break;
                }
            }

            // Expect )) - both closing parens
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        }

        Ok(attributes)
    }
}
