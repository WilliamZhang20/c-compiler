use model::{Token, Type};
use crate::parser::Parser;

/// Type parsing functionality
pub(crate) trait TypeParser {
    fn parse_type(&mut self) -> Result<Type, String>;
    fn parse_struct_definition(&mut self) -> Result<model::StructDef, String>;
}

impl<'a> TypeParser for Parser<'a> {
    fn parse_type(&mut self) -> Result<Type, String> {
        // Skip modifiers
        loop {
            if self.match_token(|t| {
                matches!(
                    t,
                    Token::Static
                        | Token::Extern
                        | Token::Inline
                        | Token::Const
                        | Token::Restrict
                        | Token::Attribute
                        | Token::Extension
                )
            }) {
                if let Some(Token::Attribute | Token::Extension) = self.previous() {
                    self.skip_parentheses()?;
                }
            } else {
                break;
            }
        }

        let mut base = if let Some(Token::Identifier { value }) = self.peek() {
            if self.typedefs.contains(value) {
                let v = value.clone();
                self.advance();
                Type::Typedef(v)
            } else {
                return Err(format!("expected type specifier, found identifier {:?}", value));
            }
        } else {
            match self.advance() {
                Some(Token::Int) => Type::Int,
                Some(Token::Void) => Type::Void,
                Some(Token::Char) => Type::Char,
                Some(Token::Struct) => self.parse_struct_type()?,
                other => return Err(format!("expected type specifier, found {:?}", other)),
            }
        };

        // Handle pointer types
        while self.match_token(|t| matches!(t, Token::Star)) {
            base = Type::Pointer(Box::new(base));
        }

        Ok(base)
    }

    fn parse_struct_definition(&mut self) -> Result<model::StructDef, String> {
        self.expect(|t| matches!(t, Token::Struct), "struct")?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected struct name identifier, found {:?}", other)),
        };
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;

        let mut fields = Vec::new();
        while !self.check(&|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            let ty = self.parse_type()?;
            let field_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected field name, found {:?}", other)),
            };

            // Handle optional array in struct field
            let final_ty = if self.match_token(|t| matches!(t, Token::OpenBracket)) {
                let size = match self.advance() {
                    Some(Token::Constant { value }) => *value as usize,
                    other => return Err(format!("expected constant array size, found {:?}", other)),
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                Type::Array(Box::new(ty), size)
            } else {
                ty
            };

            fields.push((final_ty, field_name));
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        }

        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        Ok(model::StructDef { name, fields })
    }
}

impl<'a> Parser<'a> {
    fn parse_struct_type(&mut self) -> Result<Type, String> {
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected struct tag, found {:?}", other)),
        };
        Ok(Type::Struct(name))
    }

    fn skip_parentheses(&mut self) -> Result<(), String> {
        if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
            return Ok(());
        }
        let mut depth = 1;
        while depth > 0 && !self.is_at_end() {
            match self.advance() {
                Some(Token::OpenParenthesis) => depth += 1,
                Some(Token::CloseParenthesis) => depth -= 1,
                None => break,
                _ => {}
            }
        }
        Ok(())
    }
}
