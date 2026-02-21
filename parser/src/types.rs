use model::{Token, Type, TypeQualifiers, Expr};
use crate::parser::Parser;
use crate::attributes::AttributeParser;
use crate::expressions::ExpressionParser;
use crate::utils::ParserUtils;

/// Type parsing functionality
pub(crate) trait TypeParser {
    fn parse_type(&mut self) -> Result<Type, String>;
    fn parse_type_with_qualifiers(&mut self) -> Result<(Type, TypeQualifiers), String>;
    fn parse_struct_definition(&mut self) -> Result<model::StructDef, String>;
    fn parse_union_definition(&mut self) -> Result<model::UnionDef, String>;
    fn parse_enum_definition(&mut self) -> Result<model::EnumDef, String>;
}

impl<'a> TypeParser for Parser<'a> {
    fn parse_type(&mut self) -> Result<Type, String> {
        let (ty, _qualifiers) = self.parse_type_with_qualifiers()?;
        Ok(ty)
    }

    fn parse_type_with_qualifiers(&mut self) -> Result<(Type, TypeQualifiers), String> {
        let mut qualifiers = TypeQualifiers::default();

        // Parse storage class specifiers and type qualifiers
        loop {
            let token = self.peek();
            match token {
                Some(Token::Static | Token::Extern) => {
                    self.advance();
                }
                Some(Token::Inline) => {
                    self.advance();
                }
                Some(Token::Const) => {
                    qualifiers.is_const = true;
                    self.advance();
                }
                Some(Token::Volatile) => {
                    qualifiers.is_volatile = true;
                    self.advance();
                }
                Some(Token::Restrict) => {
                    qualifiers.is_restrict = true;
                    self.advance();
                }
                Some(Token::Attribute | Token::Extension) => {
                    self.advance();
                    if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                        self.skip_parentheses()?;
                    }
                }
                _ => break,
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
                Some(Token::Bool) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify '_Bool' type".to_string());
                    }
                    base_type = Some(Type::Bool);
                    self.advance();
                }
                Some(Token::Register) => {
                    // 'register' storage class — just skip it
                    self.advance();
                }
                Some(Token::Struct) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify struct type".to_string());
                    }
                    self.advance();
                    let (struct_type, _) = self.parse_struct_type()?;
                    return Ok((struct_type, qualifiers));
                }
                Some(Token::Union) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify union type".to_string());
                    }
                    self.advance();
                    let (union_type, _) = self.parse_union_type()?;
                    return Ok((union_type, qualifiers));
                }
                Some(Token::Enum) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify enum type".to_string());
                    }
                    self.advance();
                    let (enum_type, _) = self.parse_enum_type()?;
                    return Ok((enum_type, qualifiers));
                }
                Some(Token::Typeof) => {
                    if is_unsigned || is_signed || long_count > 0 || is_short {
                        return Err("Cannot modify typeof type".to_string());
                    }
                    self.advance();
                    self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                    // Peek ahead: if it looks like a type, parse as typeof(type)
                    // otherwise parse as typeof(expr)
                    let ty = if self.check_is_type() {
                        self.parse_type()?
                    } else {
                        let expr = self.parse_expr()?;
                        Type::TypeofExpr(Box::new(expr))
                    };
                    self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                    return Ok((ty, qualifiers));
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
            // Skip qualifiers after * (e.g., int * restrict p)
            while self.match_token(|t| matches!(t, Token::Const | Token::Volatile | Token::Restrict)) {
                // Qualifiers after * apply to the pointer itself
                // For now, we just skip them (not tracked per-pointer-level)
            }
            final_type = Type::Pointer(Box::new(final_type));
        }

        Ok((final_type, qualifiers))
    }

    fn parse_struct_definition(&mut self) -> Result<model::StructDef, String> {
        self.expect(|t| matches!(t, Token::Struct), "struct")?;
        
        // Parse attributes before struct name (e.g., struct __attribute__((packed)) foo)
        let mut attributes = self.parse_attributes()?;
        
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected struct name identifier, found {:?}", other)),
        };
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;

        let mut fields = Vec::new();
        while !self.check(|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            // Try to parse field type - if it fails, skip to next semicolon or closing brace
            let ty = match self.parse_type() {
                Ok(t) => t,
                Err(_) => {
                    // Failed to parse type (e.g., unknown typedef from headers)
                    // Skip to next semicolon to continue parsing other fields
                    while !self.is_at_end() 
                        && !self.check(|t| matches!(t, Token::Semicolon)) 
                        && !self.check(|t| matches!(t, Token::CloseBrace)) {
                        self.advance();
                    }
                    if self.check(|t| matches!(t, Token::Semicolon)) {
                        self.advance();
                    }
                    continue; // Skip this field and try next one
                }
            };
            
            let field_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                _ => {
                    // Skip to next semicolon or closing brace
                    while !self.is_at_end() 
                        && !self.check(|t| matches!(t, Token::Semicolon)) 
                        && !self.check(|t| matches!(t, Token::CloseBrace)) {
                        self.advance();
                    }
                    if self.check(|t| matches!(t, Token::Semicolon)) {
                        self.advance();
                    }
                    continue; // Skip this field
                }
            };

            // Handle optional array in struct field (supports multi-dimensional)
            let mut final_ty = ty;
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are allowed)
                let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                    0 // Use 0 to represent unsized array
                } else {
                    match self.parse_array_size() {
                        Ok(s) => s,
                        Err(_) => {
                            // Skip malformed array
                            while !self.is_at_end() 
                                && !self.check(|t| matches!(t, Token::Semicolon)) 
                                && !self.check(|t| matches!(t, Token::CloseBrace)) {
                                self.advance();
                            }
                            if self.check(|t| matches!(t, Token::Semicolon)) {
                                self.advance();
                            }
                            continue; // Skip this field
                        }
                    }
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                final_ty = Type::Array(Box::new(final_ty), size);
            }

            // Check for bit field syntax (: width)
            let bit_width = if self.match_token(|t| matches!(t, Token::Colon)) {
                match self.advance() {
                    Some(Token::Constant { value }) => Some(*value as usize),
                    _ => {
                        // Skip malformed bit field
                        while !self.is_at_end() 
                            && !self.check(|t| matches!(t, Token::Semicolon)) 
                            && !self.check(|t| matches!(t, Token::CloseBrace)) {
                            self.advance();
                        }
                        if self.check(|t| matches!(t, Token::Semicolon)) {
                            self.advance();
                        }
                        continue; // Skip this field
                    }
                }
            } else {
                None
            };

            fields.push(model::StructField {
                field_type: final_ty,
                name: field_name,
                bit_width,
            });
            
            if self.expect(|t| matches!(t, Token::Semicolon), "';'").is_err() {
                // Failed to find semicolon - skip to next one or closing brace
                while !self.is_at_end() 
                    && !self.check(|t| matches!(t, Token::Semicolon)) 
                    && !self.check(|t| matches!(t, Token::CloseBrace)) {
                    self.advance();
                }
                if self.check(|t| matches!(t, Token::Semicolon)) {
                    self.advance();
                }
            }
        }

        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        
        // Parse attributes after struct body (e.g., struct foo { ... } __attribute__((packed)))
        let mut more_attributes = self.parse_attributes()?;
        attributes.append(&mut more_attributes);
        
        Ok(model::StructDef { name, fields, attributes })
    }

    fn parse_union_definition(&mut self) -> Result<model::UnionDef, String> {
        self.expect(|t| matches!(t, Token::Union), "union")?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected union name identifier, found {:?}", other)),
        };
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;

        let mut fields = Vec::new();
        while !self.check(|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            // Try to parse field type - if it fails, skip to next semicolon or closing brace
            let ty = match self.parse_type() {
                Ok(t) => t,
                Err(_) => {
                    // Failed to parse type (e.g., unknown typedef from headers)
                    // Skip to next semicolon to continue parsing other fields
                    while !self.is_at_end() 
                        && !self.check(|t| matches!(t, Token::Semicolon)) 
                        && !self.check(|t| matches!(t, Token::CloseBrace)) {
                        self.advance();
                    }
                    if self.check(|t| matches!(t, Token::Semicolon)) {
                        self.advance();
                    }
                    continue; // Skip this field and try next one
                }
            };
            
            let field_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                _ => {
                    // Skip to next semicolon or closing brace
                    while !self.is_at_end() 
                        && !self.check(|t| matches!(t, Token::Semicolon)) 
                        && !self.check(|t| matches!(t, Token::CloseBrace)) {
                        self.advance();
                    }
                    if self.check(|t| matches!(t, Token::Semicolon)) {
                        self.advance();
                    }
                    continue; // Skip this field
                }
            };

            // Handle optional array in union field (supports multi-dimensional)
            let mut final_ty = ty;
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are allowed)
                let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                    0 // Use 0 to represent unsized array
                } else {
                    match self.parse_array_size() {
                        Ok(s) => s,
                        Err(_) => {
                            // Skip malformed array
                            while !self.is_at_end() 
                                && !self.check(|t| matches!(t, Token::Semicolon)) 
                                && !self.check(|t| matches!(t, Token::CloseBrace)) {
                                self.advance();
                            }
                            if self.check(|t| matches!(t, Token::Semicolon)) {
                                self.advance();
                            }
                            continue; // Skip this field
                        }
                    }
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                final_ty = Type::Array(Box::new(final_ty), size);
            }

            fields.push(model::StructField {
                field_type: final_ty,
                name: field_name,
                bit_width: None, // Unions don't support bit fields
            });
            
            if self.expect(|t| matches!(t, Token::Semicolon), "';'").is_err() {
                // Failed to find semicolon - skip to next one or closing brace
                while !self.is_at_end() 
                    && !self.check(|t| matches!(t, Token::Semicolon)) 
                    && !self.check(|t| matches!(t, Token::CloseBrace)) {
                    self.advance();
                }
                if self.check(|t| matches!(t, Token::Semicolon)) {
                    self.advance();
                }
            }
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

        while !self.check(|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
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
    fn parse_struct_type(&mut self) -> Result<(Type, TypeQualifiers), String> {
        // Skip attributes before struct name
        let _ = self.parse_attributes()?;
        
        // Allow anonymous structs (no tag name)
        let name = if let Some(Token::Identifier { value }) = self.peek() {
            let n = value.clone();
            self.advance();
            n
        } else {
            // Anonymous struct - use empty string or generate unique name
            String::new()
        };
        Ok((Type::Struct(name), TypeQualifiers::default()))
    }

    fn parse_union_type(&mut self) -> Result<(Type, TypeQualifiers), String> {
        // Skip attributes before union name
        let _ = self.parse_attributes()?;
        
        // Allow anonymous unions (no tag name)
        let name = if let Some(Token::Identifier { value }) = self.peek() {
            let n = value.clone();
            self.advance();
            n
        } else {
            // Anonymous union - use empty string or generate unique name
            String::new()
        };
        Ok((Type::Union(name), TypeQualifiers::default()))
    }

    fn parse_enum_type(&mut self) -> Result<(Type, TypeQualifiers), String> {
        // For "enum Name", just treat as int (C standard behavior)
        if let Some(Token::Identifier { .. }) = self.peek() {
            self.advance();
        }
        Ok((Type::Int, TypeQualifiers::default()))
    }

    pub(crate) fn skip_parentheses(&mut self) -> Result<(), String> {
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
