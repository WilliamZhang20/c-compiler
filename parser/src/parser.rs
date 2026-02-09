use model::{Function, GlobalVar, Program, Token};
use crate::types::TypeParser;
use crate::statements::StatementParser;
use crate::expressions::ExpressionParser;

/// Core parser struct that maintains parsing state
pub(crate) struct Parser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) pos: usize,
    pub(crate) typedefs: Vec<String>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            typedefs: Vec::new(),
        }
    }

    /// Parse the entire program (functions, globals, structs, unions, enums)
    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        let mut globals = Vec::new();
        let mut structs = Vec::new();
        let mut unions = Vec::new();
        let mut enums = Vec::new();

        while !self.is_at_end() {
            if self.match_token(|t| matches!(t, Token::Typedef)) {
                self.parse_typedef()?;
            } else if self.check(&|t| matches!(t, Token::Enum))
                && self.check_at(2, &|t: &Token| matches!(t, Token::OpenBrace))
            {
                // enum definition: enum Color { ... };
                enums.push(self.parse_enum_definition()?);
                self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            } else if self.is_function_definition() {
                functions.push(self.parse_function()?);
            } else if self.is_function_declaration() {
                // Function prototype/declaration - just skip it
                // The actual definition will come from another file or later
                self.skip_function_declaration()?;
            } else if self.check_is_type() {
                // Could be a global declaration, struct definition, or union definition
                if self.check(&|t| matches!(t, Token::Struct)) && self.is_struct_definition() {
                    // struct definition without variable: struct foo { ... };
                    structs.push(self.parse_struct_definition()?);
                    self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
                } else if self.check(&|t| matches!(t, Token::Union)) && self.is_union_definition() {
                    // union definition without variable: union foo { ... };
                    unions.push(self.parse_union_definition()?);
                    self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
                } else {
                    globals.push(self.parse_global()?);
                }
            } else {
                // If not function and not type (e.g. typedef, struct, etc.), skip
                self.skip_top_level_item()?;
            }
        }

        Ok(Program {
            functions,
            globals,
            structs,
            unions,
            enums,
        })
    }

    fn parse_typedef(&mut self) -> Result<(), String> {
        let _ty = self.parse_type()?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => {
                return Err(format!(
                    "expected identifier for typedef name, found {:?}",
                    other
                ))
            }
        };
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        self.typedefs.push(name);
        Ok(())
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        // Track inline and attributes before parsing type
        let saved_pos = self.pos;
        let mut is_inline = false;
        
        // Scan for inline keyword
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::Inline) => {
                    is_inline = true;
                    break;
                }
                Some(Token::Static | Token::Extern | Token::Const | Token::Volatile | Token::Restrict) => {
                    self.pos += 1;
                }
                Some(Token::Attribute | Token::Extension) => {
                    self.pos += 1;
                    if self.check(&|t| matches!(t, Token::OpenParenthesis)) {
                        let _ = self.skip_parentheses();
                    }
                }
                _ => break,
            }
        }
        
        // Reset position
        self.pos = saved_pos;
        
        // Parse attributes before function
        let mut attributes = self.parse_attributes()?;
        
        let return_type = self.parse_type()?;
        
        // Parse attributes after return type but before function name
        let mut more_attributes = self.parse_attributes()?;
        attributes.append(&mut more_attributes);
        
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => {
                return Err(format!(
                    "expected function name identifier, found {:?}",
                    other
                ))
            }
        };

        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let params = self.parse_function_params()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        
        // Parse attributes after function declaration (e.g., void foo() __attribute__((noreturn)))
        let mut post_attributes = self.parse_attributes()?;
        attributes.append(&mut post_attributes);

        let body_block = self.parse_block()?;

        Ok(Function {
            return_type,
            name,
            params,
            body: body_block,
            is_inline,
            attributes,
        })
    }

    fn parse_function_params(&mut self) -> Result<Vec<(model::Type, String)>, String> {
        let mut params = Vec::new();

        if self.check(&|t| matches!(t, Token::CloseParenthesis)) {
            return Ok(params);
        }

        loop {
            if self.match_token(|t| matches!(t, Token::Ellipsis)) {
                // Just skip ellipsis for now
                if !self.match_token(|t| matches!(t, Token::Comma)) {
                    break;
                }
                continue;
            }

            let p_type = self.parse_type()?;
            let p_name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected parameter name, found {:?}", other)),
            };
            params.push((p_type, p_name));

            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }

        Ok(params)
    }

    fn parse_global(&mut self) -> Result<GlobalVar, String> {
        // Parse attributes before the type
        let mut attributes = self.parse_attributes()?;
        
        let (mut r#type, qualifiers) = self.parse_type_with_qualifiers()?;
        
        // Parse attributes after the type but before the identifier
        let mut more_attributes = self.parse_attributes()?;
        attributes.append(&mut more_attributes);
        
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected identifier after type, found {:?}", other)),
        };

        // Check for array
        if self.match_token(|t| matches!(t, Token::OpenBracket)) {
            let size = match self.advance() {
                Some(Token::Constant { value }) => *value as usize,
                other => return Err(format!("expected constant array size, found {:?}", other)),
            };
            self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
            r#type = model::Type::Array(Box::new(r#type), size);
        }

        let init = if self.match_token(|t| matches!(t, Token::Equal)) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

        Ok(GlobalVar {
            r#type,
            qualifiers,
            name,
            init,
            attributes,
        })
    }

    /// Lookahead to determine if this is a function definition (vs declaration)
    fn is_function_definition(&self) -> bool {
        let mut temp_pos = self.pos;

        // Skip modifiers
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            if matches!(
                tok,
                Token::Static
                    | Token::Extern
                    | Token::Inline
                    | Token::Attribute
                    | Token::Extension
                    | Token::Const
                    | Token::Volatile
                    | Token::Restrict
                    | Token::Hash
            ) {
                temp_pos += 1;
                // If it's attribute or extension, it might have parentheses
                if temp_pos < self.tokens.len()
                    && matches!(self.tokens[temp_pos], Token::OpenParenthesis)
                {
                    temp_pos = self.skip_parentheses_from(temp_pos);
                }
            } else {
                break;
            }
        }

        if temp_pos >= self.tokens.len() {
            return false;
        }

        // Must start with a known type
        if !(matches!(
            self.tokens[temp_pos],
            Token::Int | Token::Void | Token::Char | Token::Struct | Token::Float | Token::Double
        ) || (if let Token::Identifier { value } = &self.tokens[temp_pos] {
            self.typedefs.contains(value)
        } else {
            false
        })) {
            return false;
        }

        if matches!(self.tokens[temp_pos], Token::Struct) {
            temp_pos += 1; // skip struct
            if temp_pos < self.tokens.len()
                && matches!(self.tokens[temp_pos], Token::Identifier { .. })
            {
                temp_pos += 1; // skip tag
            }
        } else {
            temp_pos += 1;
        }

        // Followed by identifier or star (for pointers)
        while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Star) {
            temp_pos += 1;
        }

        // Skip attributes between type and function name
        while temp_pos < self.tokens.len() {
            if matches!(self.tokens[temp_pos], Token::Attribute | Token::Extension) {
                temp_pos += 1;
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
                    temp_pos = self.skip_parentheses_from(temp_pos);
                }
            } else {
                break;
            }
        }

        if temp_pos >= self.tokens.len() {
            return false;
        }
        if !matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            return false;
        }
        temp_pos += 1;

        // Followed by '('
        if temp_pos >= self.tokens.len()
            || !matches!(self.tokens[temp_pos], Token::OpenParenthesis)
        {
            return false;
        }

        // Search for '{' or ';' to distinguish definition vs prototype
        let mut paren_depth = 0;
        while temp_pos < self.tokens.len() {
            match &self.tokens[temp_pos] {
                Token::OpenParenthesis => paren_depth += 1,
                Token::CloseParenthesis => paren_depth -= 1,
                Token::OpenBrace if paren_depth == 0 => return true,
                Token::Semicolon if paren_depth == 0 => return false,
                _ => {}
            }
            temp_pos += 1;
        }
        false
    }

    /// Check if this is a function declaration (prototype) with semicolon
    fn is_function_declaration(&self) -> bool {
        let mut temp_pos = self.pos;

        // Skip modifiers
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            if matches!(
                tok,
                Token::Static
                    | Token::Extern
                    | Token::Inline
                    | Token::Attribute
                    | Token::Extension
                    | Token::Const
                    | Token::Volatile
                    | Token::Restrict
            ) {
                temp_pos += 1;
                if temp_pos < self.tokens.len()
                    && matches!(self.tokens[temp_pos], Token::OpenParenthesis)
                {
                    temp_pos = self.skip_parentheses_from(temp_pos);
                }
            } else {
                break;
            }
        }

        if temp_pos >= self.tokens.len() {
            return false;
        }

        // Must start with a known type
        if !(matches!(
            self.tokens[temp_pos],
            Token::Int | Token::Void | Token::Char | Token::Struct | Token::Float | Token::Double | Token::Long | Token::Short | Token::Unsigned | Token::Signed
        ) || (if let Token::Identifier { value } = &self.tokens[temp_pos] {
            self.typedefs.contains(value)
        } else {
            false
        })) {
            return false;
        }

        if matches!(self.tokens[temp_pos], Token::Struct) {
            temp_pos += 1;
            if temp_pos < self.tokens.len()
                && matches!(self.tokens[temp_pos], Token::Identifier { .. })
            {
                temp_pos += 1;
            }
        } else {
            temp_pos += 1;
        }

        // Followed by identifier or star (for pointers)
        while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Star) {
            temp_pos += 1;
        }

        if temp_pos >= self.tokens.len() {
            return false;
        }
        if !matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            return false;
        }
        temp_pos += 1;

        // Followed by '('
        if temp_pos >= self.tokens.len()
            || !matches!(self.tokens[temp_pos], Token::OpenParenthesis)
        {
            return false;
        }

        // Search for ';' (declaration) but NOT '{' (definition)
        let mut paren_depth = 0;
        while temp_pos < self.tokens.len() {
            match &self.tokens[temp_pos] {
                Token::OpenParenthesis => paren_depth += 1,
                Token::CloseParenthesis => paren_depth -= 1,
                Token::OpenBrace if paren_depth == 0 => return false,
                Token::Semicolon if paren_depth == 0 => return true,
                _ => {}
            }
            temp_pos += 1;
        }
        false
    }

    /// Skip a function declaration (prototype)
    fn skip_function_declaration(&mut self) -> Result<(), String> {
        // Skip everything until semicolon at depth 0
        let mut paren_depth = 0;
        while !self.is_at_end() {
            match self.peek() {
                Some(Token::OpenParenthesis) => paren_depth += 1,
                Some(Token::CloseParenthesis) => paren_depth -= 1,
                Some(Token::Semicolon) if paren_depth == 0 => {
                    self.advance(); // consume semicolon
                    return Ok(());
                }
                _ => {}
            }
            self.advance();
        }
        Err("Unexpected end of file in function declaration".to_string())
    }

    fn skip_parentheses_from(&self, start_pos: usize) -> usize {
        let mut depth = 1;
        let mut pos = start_pos + 1;
        while depth > 0 && pos < self.tokens.len() {
            match self.tokens[pos] {
                Token::OpenParenthesis => depth += 1,
                Token::CloseParenthesis => depth -= 1,
                _ => {}
            }
            pos += 1;
        }
        pos
    }
    
    /// Check if current position is a struct definition (not just a declaration)
    /// Handles attributes like: struct __attribute__((packed)) Foo { ... };
    fn is_struct_definition(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'struct'
        
        // Skip attributes
        while temp_pos < self.tokens.len() {
            if matches!(self.tokens[temp_pos], Token::Attribute | Token::Extension) {
                temp_pos += 1;
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
                    temp_pos = self.skip_parentheses_from(temp_pos);
                }
            } else {
                break;
            }
        }
        
        // Skip struct name
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        }
        
        // Check for '{'
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace)
    }
    
    /// Check if current position is a union definition (not just a declaration)
    fn is_union_definition(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'union'
        
        // Skip attributes
        while temp_pos < self.tokens.len() {
            if matches!(self.tokens[temp_pos], Token::Attribute | Token::Extension) {
                temp_pos += 1;
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
                    temp_pos = self.skip_parentheses_from(temp_pos);
                }
            } else {
                break;
            }
        }
        
        // Skip union name
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        }
        
        // Check for '{'
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace)
    }

    fn skip_top_level_item(&mut self) -> Result<(), String> {
        // Simple panic mode recovery: skip until semicolon or brace
        while !self.is_at_end() {
            match self.peek() {
                Some(Token::Semicolon) => {
                    self.advance();
                    return Ok(());
                }
                Some(Token::OpenBrace) => {
                    self.skip_block_internal()?;
                    return Ok(());
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(())
    }

    fn skip_block_internal(&mut self) -> Result<(), String> {
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;
        let mut depth = 1;
        while depth > 0 && !self.is_at_end() {
            match self.advance() {
                Some(Token::OpenBrace) => depth += 1,
                Some(Token::CloseBrace) => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn check_is_type(&self) -> bool {
        self.check_is_type_at(0)
    }

    pub(crate) fn check_is_type_at(&self, offset: usize) -> bool {
        match self.tokens.get(self.pos + offset) {
            Some(
                Token::Int
                | Token::Void
                | Token::Char
                | Token::Float
                | Token::Double
                | Token::Struct
                | Token::Union
                | Token::Enum
                | Token::Unsigned
                | Token::Signed
                | Token::Long
                | Token::Short
                | Token::Static
                | Token::Extern
                | Token::Inline
                | Token::Const
                | Token::Volatile
                | Token::Restrict
                | Token::Attribute
                | Token::Extension,
            ) => true,
            Some(Token::Identifier { value }) => self.typedefs.contains(value),
            _ => false,
        }
    }

    // Token navigation utilities
    pub(crate) fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub(crate) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub(crate) fn previous(&self) -> Option<&Token> {
        if self.pos == 0 {
            None
        } else {
            self.tokens.get(self.pos - 1)
        }
    }

    pub(crate) fn advance(&mut self) -> Option<&Token> {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.previous()
    }

    pub(crate) fn match_token<F>(&mut self, predicate: F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        if self.check(&predicate) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn check<F>(&self, predicate: &F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        match self.peek() {
            Some(tok) => predicate(tok),
            None => false,
        }
    }

    pub(crate) fn check_at<F>(&self, offset: usize, predicate: F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        self.tokens.get(self.pos + offset).map_or(false, predicate)
    }

    pub(crate) fn expect<F>(&mut self, predicate: F, expected: &str) -> Result<(), String>
    where
        F: Fn(&Token) -> bool,
    {
        if self.check(&predicate) {
            self.advance();
            Ok(())
        } else {
            Err(format!("expected {expected}, found {:?}", self.peek()))
        }
    }

    /// Parse __attribute__((...)) syntax and return a list of attributes
    pub(crate) fn parse_attributes(&mut self) -> Result<Vec<model::Attribute>, String> {
        use model::Attribute;
        let mut attributes = Vec::new();

        while self.match_token(|t| matches!(t, Token::Attribute | Token::Extension)) {
            // Expect (( after __attribute__
            if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                continue; // Just skip if no parentheses
            }
            if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                return Err("expected '(' after '__attribute__(('".to_string());
            }

            // Parse attributes inside, comma-separated
            loop {
                if self.check(&|t| matches!(t, Token::CloseParenthesis)) {
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
                                Some(Token::Constant { value }) => {
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
                    _ => {
                        // Skip unknown attributes
                        self.advance();
                        if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                            self.skip_parentheses()?;
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
