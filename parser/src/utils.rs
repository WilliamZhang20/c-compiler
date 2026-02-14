use model::Token;
use crate::parser::Parser;
use crate::types::TypeParser;

pub(crate) trait ParserUtils {
    fn is_function_definition(&self) -> bool;
    fn is_inline_function(&self) -> bool;
    fn skip_extern_inline_function(&mut self) -> Result<(), String>;
    fn is_function_declaration(&self) -> bool;
    fn skip_function_declaration(&mut self) -> Result<(), String>;
    fn skip_extern_declaration(&mut self) -> Result<(), String>;
    fn skip_parentheses_from(&self, start_pos: usize) -> usize;
    fn skip_block_from(&self, start_pos: usize) -> usize;
    fn is_struct_definition(&self) -> bool;
    fn is_union_definition(&self) -> bool;
    fn is_struct_forward_declaration(&self) -> bool;
    fn is_union_forward_declaration(&self) -> bool;
    fn skip_forward_declaration(&mut self) -> Result<(), String>;
    fn skip_top_level_item(&mut self) -> Result<(), String>;
    fn skip_block_internal(&mut self) -> Result<(), String>;
    fn skip_parentheses_content(&mut self) -> Result<(), String>;
    fn check_is_type(&self) -> bool;
    fn check_is_type_at(&self, offset: usize) -> bool;
}

impl<'a> ParserUtils for Parser<'a> {
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

        // Skip type specifiers (handle multi-token types like 'unsigned int')
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            if matches!(
                tok,
                Token::Int | Token::Void | Token::Char | Token::Float | Token::Double | Token::Long | Token::Short | Token::Unsigned | Token::Signed
            ) {
                temp_pos += 1;
            } else if matches!(tok, Token::Struct | Token::Union | Token::Enum) {
                temp_pos += 1; // skip struct/union/enum
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
                    temp_pos += 1; // skip tag
                }
                // Also check for struct definition {}
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace) {
                    temp_pos = self.skip_block_from(temp_pos);
                }
            } else if let Token::Identifier { value } = tok {
                if self.typedefs.contains(value) {
                    temp_pos += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Followed by identifier or star (for pointers)
        while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Star) {
            temp_pos += 1;
            // Skip qualifiers after *
            while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Const | Token::Volatile | Token::Restrict) {
                temp_pos += 1;
            }
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

    /// Check if this is an extern inline function (from headers)
    fn is_inline_function(&self) -> bool {
        let mut temp_pos = self.pos;
        let mut has_inline = false;
        let mut has_extern = false;

        // Scan modifiers
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            match tok {
                Token::Inline => {
                    has_inline = true;
                    temp_pos += 1;
                }
                Token::Extern => {
                    has_extern = true;
                    temp_pos += 1;
                }
                Token::Static | Token::Const | Token::Volatile | Token::Restrict | Token::Extension => {
                    temp_pos += 1;
                }
                Token::Attribute => {
                    temp_pos += 1;
                    if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
                        temp_pos = self.skip_parentheses_from(temp_pos);
                    }
                }
                _ => break,
            }
        }

        // Must have BOTH extern AND inline (header functions) and be a function definition (with body)
        has_inline && has_extern && self.is_function_definition()
    }

    /// Skip an extern inline function definition
    fn skip_extern_inline_function(&mut self) -> Result<(), String> {
        // Skip modifiers and type
        while self.check(|t| matches!(t, Token::Extern | Token::Inline | Token::Static | Token::Const | Token::Volatile | Token::Restrict | Token::Extension | Token::Attribute)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        // Skip type
        self.parse_type()?;
        
        // Skip attributes after type
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        // Skip function name
        if self.check(|t| matches!(t, Token::Identifier { .. })) {
            self.advance();
        }
        
        // Skip parameters
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let mut depth = 1;
        while depth > 0 && !self.is_at_end() {
            match self.peek() {
                Some(Token::OpenParenthesis) => depth += 1,
                Some(Token::CloseParenthesis) => depth -= 1,
                _ => {}
            }
            self.advance();
        }
        
        // Skip attributes after parameters
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        // Skip function body
        self.skip_block_internal()?;
        
        Ok(())
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

        // Skip type specifiers (handle multi-token types like 'unsigned int')
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            if matches!(
                tok,
                Token::Int | Token::Void | Token::Char | Token::Float | Token::Double | Token::Long | Token::Short | Token::Unsigned | Token::Signed
            ) {
                temp_pos += 1;
            } else if matches!(tok, Token::Struct | Token::Union | Token::Enum) {
                temp_pos += 1; // skip struct/union/enum
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
                    temp_pos += 1; // skip tag
                }
                // Also check for struct definition {}
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace) {
                    temp_pos = self.skip_block_from(temp_pos);
                }
            } else if let Token::Identifier { value } = tok {
                if self.typedefs.contains(value) {
                    temp_pos += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Followed by identifier or star (for pointers)
        while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Star) {
            temp_pos += 1;
            // Skip qualifiers after *
            while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Const | Token::Volatile | Token::Restrict) {
                temp_pos += 1;
            }
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

    fn skip_function_declaration(&mut self) -> Result<(), String> {
        let mut paren_depth = 0;
        while !self.is_at_end() {
            match self.peek() {
                Some(Token::OpenParenthesis) => paren_depth += 1,
                Some(Token::CloseParenthesis) => paren_depth -= 1,
                Some(Token::Semicolon) if paren_depth == 0 => {
                    self.advance();
                    return Ok(());
                }
                _ => {}
            }
            self.advance();
        }
        Err("Unexpected end of file in function declaration".to_string())
    }

    fn skip_extern_declaration(&mut self) -> Result<(), String> {
        while !self.is_at_end() {
            if self.match_token(|t| matches!(t, Token::Semicolon)) {
                return Ok(());
            }
            self.advance();
        }
        Err("Unexpected end of file in extern declaration".to_string())
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

    fn skip_block_from(&self, start_pos: usize) -> usize {
        let mut depth = 1;
        let mut pos = start_pos + 1;
        while depth > 0 && pos < self.tokens.len() {
            match self.tokens[pos] {
                Token::OpenBrace => depth += 1,
                Token::CloseBrace => depth -= 1,
                _ => {}
            }
            pos += 1;
        }
        pos
    }
    
    fn is_struct_definition(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'struct'
        
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
        
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        }
        
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace)
    }
    
    fn is_union_definition(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'union'
        
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
        
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        }
        
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenBrace)
    }

    fn is_struct_forward_declaration(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'struct'
        
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
        
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        } else {
            return false;
        }
        
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Semicolon)
    }

    fn is_union_forward_declaration(&self) -> bool {
        let mut temp_pos = self.pos + 1; // Skip 'union'
        
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
        
        if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            temp_pos += 1;
        } else {
            return false;
        }
        
        temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Semicolon)
    }

    fn skip_forward_declaration(&mut self) -> Result<(), String> {
        self.advance(); // skip struct/union keyword
        
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        if self.check(|t| matches!(t, Token::Identifier { .. })) {
            self.advance();
        }
        
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(())
    }

    fn skip_top_level_item(&mut self) -> Result<(), String> {
        while !self.is_at_end() {
            match self.peek() {
                Some(Token::Semicolon) => {
                    self.advance();
                    return Ok(());
                }
                Some(Token::OpenParenthesis) => {
                    let _ = self.skip_parentheses();
                    continue;
                }
                Some(Token::OpenBracket) => {
                    self.advance();
                    let mut depth = 1;
                    while depth > 0 && !self.is_at_end() {
                        match self.advance() {
                            Some(Token::OpenBracket) => depth += 1,
                            Some(Token::CloseBracket) => depth -= 1,
                            _ => {}
                        }
                    }
                    continue;
                }
                Some(Token::OpenBrace) => {
                    let _ = self.skip_block_internal();
                    return Ok(());
                }
                Some(Token::CloseBrace) => {
                    self.advance();
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

    fn skip_parentheses_content(&mut self) -> Result<(), String> {
        let mut depth = 1;
        while depth > 0 && !self.is_at_end() {
            match self.peek() {
                Some(Token::OpenParenthesis) => {
                    depth += 1;
                    self.advance();
                }
                Some(Token::CloseParenthesis) => {
                    depth -= 1;
                    if depth > 0 {
                        self.advance();
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(())
    }

    fn check_is_type(&self) -> bool {
        self.check_is_type_at(0)
    }

    fn check_is_type_at(&self, offset: usize) -> bool {
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
}
