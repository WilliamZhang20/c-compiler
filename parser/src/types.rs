use model::{Token, Type};
use crate::parser::Parser;

/// Type parsing functionality
pub(crate) trait TypeParser {
    fn parse_type(&mut self) -> Result<Type, String>;
    fn parse_struct_definition(&mut self) -> Result<model::StructDef, String>;
    fn parse_union_definition(&mut self) -> Result<model::UnionDef, String>;
    fn parse_enum_definition(&mut self) -> Result<model::EnumDef, String>;
}

impl<'a> TypeParser for Parser<'a> {
    fn parse_type(&mut self) -> Result<Type, String> {
        // Skip modifiers like static, extern, const, etc.
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

        // Parse type modifiers and base type
        let mut is_unsigned = false;
        let mut is_signed = false;
        let mut long_count = 0; // 0 = no long, 1 = long, 2 = long long
        let mut is_short = false;
        let mut base_type = None;

        // Collect type specifiers
        loop {
            let token = self.peek();
            match token {
                Some(Token::Unsigned) => {
                    if is_signed {
                        return Err("Cannot combine 'unsigned' and 'signed'".to_string());
                    }
                    is_unsigned = true;
                    self.advance();
                }
                Some(Token::Signed) => {
                    if is_unsigned {
                        return Err("Cannot combine 'unsigned' and 'signed'".to_string());
                    }
                    is_signed = true;
                    self.advance();
                }
                Some(Token::Long) => {
                    if is_short {
                        return Err("Cannot combine 'long' and 'short'".to_string());
                    }
                    long_count += 1;
                    if long_count > 2 {
                        return Err("Too many 'long' specifiers".to_string());
                    }
                    self.advance();
                }
                Some(Token::Short) => {
                    if long_count > 0 {
                        return Err("Cannot combine 'long' and 'short'".to_string());
                    }
                    is_short = true;
                    self.advance();
                }
                Some(Token::Int) => {
                    if base_type.is_some() {
                        return Err("Multiple base types specified".to_string());
                    }
                    base_type = Some(Type::Int);
                    self.advance();
                }
                Some(Token::Char) => {
                    if base_type.is_some() || long_count > 0 || is_short {
                        return Err("Invalid type combination with 'char'".to_string());
                    }
                    base_type = Some(Type::Char);
                    self.advance();
                }
                Some(Token::Void) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify 'void' type".to_string());
                    }
                    base_type = Some(Type::Void);
                    self.advance();
                }
                Some(Token::Float) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify 'float' type".to_string());
                    }
                    base_type = Some(Type::Float);
                    self.advance();
                }
                Some(Token::Double) => {
                    if is_unsigned || is_signed || is_short {
                        return Err("Cannot modify 'double' with unsigned/signed/short".to_string());
                    }
                    if long_count > 1 {
                        return Err("'long long double' is not valid".to_string());
                    }
                    base_type = Some(Type::Double);
                    self.advance();
                }
                Some(Token::Struct) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify struct type".to_string());
                    }
                    self.advance();
                    return self.parse_struct_type();
                }
                Some(Token::Union) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify union type".to_string());
                    }
                    self.advance();
                    return self.parse_union_type();
                }
                Some(Token::Enum) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify enum type".to_string());
                    }
                    self.advance();
                    return self.parse_enum_type();
                }
                Some(Token::Identifier { value }) if self.typedefs.contains(value) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify typedef".to_string());
                    }
                    let v = value.clone();
                    self.advance();
                    base_type = Some(Type::Typedef(v));
                    break;
                }
                _ => break,
            }
        }

        // If no base type specified, default to int for modifiers
        if base_type.is_none() && (is_unsigned || is_signed || long_count > 0 || is_short) {
            base_type = Some(Type::Int);
        }

        // Determine final type
        let mut final_type = match base_type {
            Some(Type::Int) => {
                if is_short {
                    if is_unsigned {
                        Type::UnsignedShort
                    } else {
                        Type::Short
                    }
                } else if long_count == 2 {
                    if is_unsigned {
                        Type::UnsignedLongLong
                    } else {
                        Type::LongLong
                    }
                } else if long_count == 1 {
                    if is_unsigned {
                        Type::UnsignedLong
                    } else {
                        Type::Long
                    }
                } else if is_unsigned {
                    Type::UnsignedInt
                } else {
                    Type::Int
                }
            }
            Some(Type::Char) => {
                if is_unsigned {
                    Type::UnsignedChar
                } else {
                    Type::Char
                }
            }
            Some(ty) => ty,
            None => {
                return Err("expected type specifier".to_string());
            }
        };

        // Handle pointer types
        while self.match_token(|t| matches!(t, Token::Star)) {
            final_type = Type::Pointer(Box::new(final_type));
        }

        Ok(final_type)
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

    fn parse_union_definition(&mut self) -> Result<model::UnionDef, String> {
        self.expect(|t| matches!(t, Token::Union), "union")?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected union name identifier, found {:?}", other)),
        };
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;

        let mut fields = Vec::new();
        while !self.check(&|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            let ty = self.parse_type()?;
            let field_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected field name, found {:?}", other)),
            };

            // Handle optional array in union field
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
        Ok(model::UnionDef { name, fields })
    }

    fn parse_enum_definition(&mut self) -> Result<model::EnumDef, String> {
        self.expect(|t| matches!(t, Token::Enum), "enum")?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected enum name identifier, found {:?}", other)),
        };
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;

        let mut constants = Vec::new();
        let mut next_value = 0_i64;

        while !self.check(&|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            let const_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected enum constant name, found {:?}", other)),
            };

            let value = if self.match_token(|t| matches!(t, Token::Equal)) {
                // Explicit value: RED = 10 or ERROR = -1
                let is_negative = self.match_token(|t| matches!(t, Token::Minus));
                match self.advance() {
                    Some(Token::Constant { value }) => {
                        let actual_value = if is_negative { -value } else { *value };
                        next_value = actual_value;
                        actual_value
                    }
                    other => return Err(format!("expected constant value, found {:?}", other)),
                }
            } else {
                // Auto-increment: GREEN (implicit = 0, 1, 2, ...)
                next_value
            };

            constants.push((const_name, value));
            next_value += 1;

            // Allow trailing comma
            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }

        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        Ok(model::EnumDef { name, constants })
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

    fn parse_union_type(&mut self) -> Result<Type, String> {
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected union tag, found {:?}", other)),
        };
        Ok(Type::Union(name))
    }

    fn parse_enum_type(&mut self) -> Result<Type, String> {
        // For "enum Name", just treat as int (C standard behavior)
        if let Some(Token::Identifier { .. }) = self.peek() {
            self.advance();
        }
        Ok(Type::Int)
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
