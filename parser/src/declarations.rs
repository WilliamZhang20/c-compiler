use model::{Function, GlobalVar, Program, Token};
use crate::parser::Parser;
use crate::types::TypeParser;
use crate::statements::StatementParser;
use crate::expressions::ExpressionParser;
use crate::attributes::AttributeParser;
use crate::utils::ParserUtils;

pub(crate) trait DeclarationParser {
    fn parse_program(&mut self) -> Result<Program, String>;
    fn parse_typedef(&mut self) -> Result<(), String>;
    fn parse_function(&mut self) -> Result<Function, String>;
    fn parse_function_prototype(&mut self) -> Result<model::FunctionPrototype, String>;
    fn parse_forward_struct_name(&mut self) -> Result<String, String>;
    fn parse_function_params(&mut self) -> Result<Vec<(model::Type, String)>, String>;
    fn parse_globals(&mut self) -> Result<Vec<GlobalVar>, String>;
    fn parse_static_assert(&mut self) -> Result<(), String>;
}

impl<'a> DeclarationParser for Parser<'a> {
    /// Parse the entire program (functions, globals, structs, unions, enums)
    fn parse_program(&mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        let mut globals = Vec::new();
        let mut structs = Vec::new();
        let mut unions = Vec::new();
        let mut enums = Vec::new();
        let mut prototypes = Vec::new();
        let mut forward_structs = Vec::new();

        while !self.is_at_end() {
             if self.match_token(|t| matches!(t, Token::StaticAssert)) {
                // _Static_assert(expr, "message") or _Static_assert(expr)
                self.parse_static_assert()?;
            } else if self.match_token(|t| matches!(t, Token::Typedef)) {
                // Try to parse typedef, but skip if it fails (complex header typedef)
                if self.parse_typedef().is_err() {
                    let _ = self.skip_top_level_item();
                }
            } else if self.check(|t| matches!(t, Token::Extension | Token::Attribute)) {
                // Parse top-level attributes (e.g., __attribute__((constructor)))
                // and apply them to the next declaration
                let attrs = self.parse_attributes()?;
                if !attrs.is_empty() {
                    // Look for the function or global that follows and attach attributes
                    if self.is_function_definition() {
                        match self.parse_function() {
                            Ok(mut f) => {
                                f.attributes.extend(attrs);
                                functions.push(f);
                            }
                            Err(_) => { let _ = self.skip_top_level_item(); }
                        }
                    } else if self.check_is_type() || self.check(|t| matches!(t, Token::Identifier { .. })) {
                        match self.parse_globals() {
                            Ok(mut gvars) => {
                                for g in &mut gvars {
                                    g.attributes.extend(attrs.clone());
                                }
                                globals.extend(gvars);
                            }
                            Err(_) => { let _ = self.skip_top_level_item(); }
                        }
                    }
                }
                continue;
            } else if self.check(|t| matches!(t, Token::Enum))
                && self.check_at(2, &|t: &Token| matches!(t, Token::OpenBrace))
            {
                // enum definition: enum Color { ... };
                // Try to parse, skip if it fails
                match self.parse_enum_definition() {
                    Ok(e) => {
                        enums.push(e);
                        let _ = self.expect(|t| matches!(t, Token::Semicolon), "';'");
                    }
                    Err(_) => {
                        let _ = self.skip_top_level_item();
                    }
                }
            } else if self.peek() == Some(&Token::Extern) {
                // Extern declarations: parse as globals with is_extern=true
                // If it's a function declaration like "extern int foo(void);", 
                // parse_globals will handle it — we fall back to skip on error.
                match self.parse_globals() {
                    Ok(gvars) => globals.extend(gvars),
                    Err(_) => { let _ = self.skip_extern_declaration(); }
                }
            } else if self.is_inline_function() {
                // Skip extern/static inline functions from headers (e.g., printf/scanf wrappers)
                let _ = self.skip_extern_inline_function();
            } else if self.is_function_definition() {
                // Try to parse function, skip if it fails
                match self.parse_function() {
                    Ok(f) => functions.push(f),
                    Err(_) => {
                        // Skip malformed function
                        if self.skip_top_level_item().is_err() {
                            // If skip also fails, just advance one token
                            self.advance();
                        }
                    }
                }
            } else if self.is_function_declaration() {
                // Function prototype/declaration - parse and store
                match self.parse_function_prototype() {
                    Ok(proto) => prototypes.push(proto),
                    Err(_) => { let _ = self.skip_function_declaration(); }
                }
            } else if self.check_is_type() 
                || self.check(|t| matches!(t, Token::Identifier { .. })) 
            {
                // Could be a global declaration, struct definition, or union definition
                // Wrap in error handling to skip complex header constructs we can't parse
                let parse_result = if self.check(|t| matches!(t, Token::Struct)) && self.is_struct_forward_declaration() {
                    // Forward struct declaration: struct foo;
                    let name = self.parse_forward_struct_name();
                    if let Ok(n) = &name {
                        forward_structs.push(n.clone());
                    }
                    name.map(|_| ())
                } else if self.check(|t| matches!(t, Token::Union)) && self.is_union_forward_declaration() {
                    // Forward union declaration: union foo;
                    self.skip_forward_declaration()
                } else if self.check(|t| matches!(t, Token::Struct)) && self.is_struct_definition() {
                    // struct definition without variable: struct foo { ... };
                    match self.parse_struct_definition() {
                        Ok(s) => {
                            structs.push(s);
                            self.expect(|t| matches!(t, Token::Semicolon), "';'")
                        }
                        Err(e) => Err(e),
                    }
                } else if self.check(|t| matches!(t, Token::Union)) && self.is_union_definition() {
                    // union definition without variable: union foo { ... };
                    match self.parse_union_definition() {
                        Ok(u) => {
                            unions.push(u);
                            self.expect(|t| matches!(t, Token::Semicolon), "';'")
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    // Try to parse as global variable(s)
                    self.parse_globals().map(|gs| globals.extend(gs))
                };
                
                // If parsing failed, skip this item
                if parse_result.is_err() {
                    let _ = self.skip_top_level_item();
                }
            } else {
                // If not function and not type (e.g. typedef, struct, etc.), skip
                let _ = self.skip_top_level_item();
            }
        }

        Ok(Program {
            functions,
            globals,
            structs,
            unions,
            enums,
            prototypes,
            forward_structs,
        })
    }

    fn parse_typedef(&mut self) -> Result<(), String> {
        let _ty = self.parse_type()?;
        
        // Check if there's an inline struct/union/enum definition
        if self.check(|t| matches!(t, Token::OpenBrace)) {
            // Skip the inline definition body
            self.skip_block_internal()?;
        }
        
        // Check for function pointer typedef: typedef int (*name)(params);
        // After parsing the base type (e.g., "int"), the next token should be
        // '(' marking the start of the function pointer declarator.
        if self.check(|t| matches!(t, Token::OpenParenthesis)) {
            // This is a function pointer typedef
            self.advance(); // consume '('
            
            // Skip attributes if present (e.g., __attribute__((__cdecl__)))
            while self.match_token(|t| matches!(t, Token::Attribute)) {
                if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                    self.skip_parentheses()?;
                }
            }
            
            // Check for '*' for pointer, if not found, skip this typedef
            if !self.match_token(|t| matches!(t, Token::Star)) {
                // Not a function pointer we understand, skip to semicolon
                while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                    self.advance();
                }
                return Ok(());
            }
            
            // Get the typedef name
            let name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                _other => {
                    // Can't parse this typedef, skip to semicolon
                    while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                        self.advance();
                    }
                    return Ok(());
                }
            };
            self.typedefs.insert(name);
            
            // Expect ')' to close the pointer declaration
            if !self.match_token(|t| matches!(t, Token::CloseParenthesis)) {
                // Malformed, skip to semicolon
                while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                    self.advance();
                }
                return Ok(());
            }
            
            // Expect '(' for parameters
            if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                // Malformed, skip to semicolon
                while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                    self.advance();
                }
                return Ok(());
            }
            
            // Skip parameters - just consume until we find matching ')'
            self.skip_parentheses_content()?;
            
            // Expect ')'
            if !self.match_token(|t| matches!(t, Token::CloseParenthesis)) {
                // Malformed, skip to semicolon
                while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                    self.advance();
                }
                return Ok(());
            }
            
            // Expect ';'
            if !self.match_token(|t| matches!(t, Token::Semicolon)) {
                // Malformed, skip to semicolon
                while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                    self.advance();
                }
            }
            return Ok(());
        }
        
        // Parse typedef aliases (can be multiple, comma-separated)
        // If we can't parse it (e.g., anonymous typedef from headers), just skip to semicolon
        loop {
            // Skip pointer stars and qualifiers
            while self.match_token(|t| matches!(t, Token::Star)) {
                // Skip any const/volatile/restrict after the star
                while self.match_token(|t| matches!(t, Token::Const | Token::Volatile | Token::Restrict)) {
                    // Just consume the qualifiers
                }
            }
            
            // Check if we have an identifier
            let name = match self.peek() {
                Some(Token::Identifier { value }) => {
                    let n = value.clone();
                    self.advance();
                    n
                }
                Some(Token::Semicolon) | _ => {
                    // No identifier (e.g., typedef struct {...}; or complex typedef we don't understand)
                    // Just skip to semicolon
                    while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                        self.advance();
                    }
                    return Ok(());
                }
            };
            self.typedefs.insert(name);
            
            // Check for array syntax: typedef int arr[10]; (supports multi-dimensional)
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are allowed)
                if !self.check(|t| matches!(t, Token::CloseBracket)) {
                    // Skip the size expression (could be constant or expression)
                    while !self.check(|t| matches!(t, Token::CloseBracket)) && !self.is_at_end() {
                        self.advance();
                    }
                }
                if !self.match_token(|t| matches!(t, Token::CloseBracket)) {
                    // Malformed array, skip to semicolon
                    while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                        self.advance();
                    }
                    return Ok(());
                }
            }
            
            // Check for comma (multiple aliases)
            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }
        
        // Final semicolon
        if !self.match_token(|t| matches!(t, Token::Semicolon)) {
            // Missing semicolon, skip to find it
            while !self.match_token(|t| matches!(t, Token::Semicolon)) && !self.is_at_end() {
                self.advance();
            }
        }
        Ok(())
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        // Track inline, static, and attributes before parsing type
        let saved_pos = self.pos;
        let mut is_inline = false;
        let mut is_static = false;
        
        // Scan for inline/static keywords
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::Inline) => {
                    is_inline = true;
                    break;
                }
                Some(Token::Static) => {
                    is_static = true;
                    self.pos += 1;
                }
                Some(Token::Extern | Token::Const | Token::Volatile | Token::Restrict) => {
                    self.pos += 1;
                }
                Some(Token::Attribute | Token::Extension) => {
                    self.pos += 1;
                    if self.check(|t| matches!(t, Token::OpenParenthesis)) {
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
        
        let return_type = match self.parse_type() {
            Ok(ty) => ty,
            Err(_) => {
                // Check if we have an identifier next (implicit int return type)
                if self.check(|t| matches!(t, Token::Identifier { .. })) {
                    model::Type::Int
                } else {
                    return Err("Expected return type or function name".to_string());
                }
            }
        };
        
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
            is_static,
            attributes,
        })
    }

    fn parse_function_prototype(&mut self) -> Result<model::FunctionPrototype, String> {
        // Skip storage class specifiers (extern, static, etc.)
        while self.check(|t| matches!(t, Token::Extern | Token::Static | Token::Inline)) {
            self.advance();
        }
        // Skip __extension__ / __attribute__
        while self.check(|t| matches!(t, Token::Extension | Token::Attribute)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        let return_type = self.parse_type()?;
        
        // Skip post-type attributes
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected function name, found {:?}", other)),
        };
        
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let params = self.parse_function_params()?;
        
        // Check for variadic
        let is_variadic = self.check(|t| matches!(t, Token::Ellipsis));
        if is_variadic {
            self.advance();
        }
        
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        
        // Skip post-declaration attributes
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        
        Ok(model::FunctionPrototype {
            return_type,
            name,
            params,
            is_variadic,
        })
    }

    fn parse_forward_struct_name(&mut self) -> Result<String, String> {
        self.advance(); // skip 'struct'
        
        // Skip attributes
        while self.check(|t| matches!(t, Token::Attribute | Token::Extension)) {
            self.advance();
            if self.check(|t| matches!(t, Token::OpenParenthesis)) {
                self.skip_parentheses()?;
            }
        }
        
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => return Err(format!("expected struct name, found {:?}", other)),
        };
        
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(name)
    }

    fn parse_function_params(&mut self) -> Result<Vec<(model::Type, String)>, String> {
        let mut params = Vec::new();

        if self.check(|t| matches!(t, Token::CloseParenthesis)) {
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

            let mut p_type = self.parse_type()?;
            
            // Handle (void)
            if matches!(p_type, model::Type::Void) && self.check(|t| matches!(t, Token::CloseParenthesis)) {
                break;
            }

            // Parameter name is optional in prototypes
            let p_name = if let Some(Token::Identifier { value }) = self.peek() {
                let name = value.clone();
                self.advance();
                name
            } else {
                "".to_string()
            };
            
            // Handle array syntax in function parameters: type name[] (supports multi-dimensional)
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are common for params)
                let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                    0 // Use 0 to represent unsized array
                } else {
                    self.parse_array_size()?
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                p_type = model::Type::Array(Box::new(p_type), size);
            }
            
            params.push((p_type, p_name));

            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }

        Ok(params)
    }

    fn parse_globals(&mut self) -> Result<Vec<GlobalVar>, String> {
        // Parse attributes before the type
        let mut attributes = self.parse_attributes()?;
        
        // Check for storage class specifiers before we parse the type (which consumes them)
        let mut is_extern = false;
        let mut is_static = false;
        {
            let mut peek_pos = self.pos;
            while peek_pos < self.tokens.len() {
                match &self.tokens[peek_pos] {
                    Token::Extern => { is_extern = true; peek_pos += 1; }
                    Token::Static => { is_static = true; peek_pos += 1; }
                    Token::Const | Token::Volatile | Token::Restrict | Token::Inline => { peek_pos += 1; }
                    Token::Attribute | Token::Extension => {
                        peek_pos += 1;
                        // Skip attribute parens
                        if peek_pos < self.tokens.len() && matches!(&self.tokens[peek_pos], Token::OpenParenthesis) {
                            let mut depth = 0;
                            while peek_pos < self.tokens.len() {
                                match &self.tokens[peek_pos] {
                                    Token::OpenParenthesis => { depth += 1; peek_pos += 1; }
                                    Token::CloseParenthesis => { depth -= 1; peek_pos += 1; if depth == 0 { break; } }
                                    _ => { peek_pos += 1; }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
        
        let (base_type, qualifiers) = match self.parse_type_with_qualifiers() {
             Ok(res) => res,
             Err(_) if self.check(|t| matches!(t, Token::Identifier { .. })) => {
                 (model::Type::Int, model::TypeQualifiers::default())
             }
             Err(e) => return Err(e),
        };
        
        // Parse attributes after the type but before the identifier
        let mut more_attributes = self.parse_attributes()?;
        attributes.append(&mut more_attributes);
        
        let mut globals = Vec::new(); // Explicit type annotation

        loop {
            let name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected identifier after type, found {:?}", other)),
            };

            let mut var_type = base_type.clone();

            // Check for array (supports multi-dimensional)
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are allowed for externs/params)
                let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                    0 // Use 0 to represent unsized array
                } else {
                    self.parse_array_size()?
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                var_type = model::Type::Array(Box::new(var_type), size);
            }

            let init = if self.match_token(|t| matches!(t, Token::Equal)) {
                if self.check(|t| matches!(t, Token::OpenBrace)) {
                    Some(self.parse_init_list()?)
                } else {
                    // Use parse_assignment (not parse_expr) so commas
                    // are treated as separators for multi-var declarations.
                    Some(self.parse_assignment()?)
                }
            } else {
                None
            };

            // Infer array size from initializer
            if let model::Type::Array(inner, 0) = &var_type {
                if let Some(model::Expr::StringLiteral(s)) = &init {
                    var_type = model::Type::Array(inner.clone(), s.len() + 1);
                } else if let Some(model::Expr::InitList(items)) = &init {
                    var_type = model::Type::Array(inner.clone(), items.len());
                }
            }
            
            globals.push(GlobalVar {
                r#type: var_type,
                qualifiers: qualifiers.clone(),
                name,
                init,
                attributes: attributes.clone(),
                is_extern,
                is_static,
            });

            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

        Ok(globals)
    }

    /// Parse _Static_assert(expr, "message") or _Static_assert(expr)
    /// The _Static_assert token has already been consumed.
    fn parse_static_assert(&mut self) -> Result<(), String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        
        // Parse the constant expression
        let expr = self.parse_assignment()?;
        
        // Check if there's a message (optional in C23)
        let _message = if self.match_token(|t| matches!(t, Token::Comma)) {
            // Consume the string literal message
            match self.advance() {
                Some(Token::StringLiteral { value }) => Some(value.clone()),
                _ => return Err("Expected string literal in _Static_assert".to_string()),
            }
        } else {
            None
        };
        
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        
        // Evaluate the expression at compile time if it's a constant
        // For non-constant expressions, we skip the check (best-effort)
        if let model::Expr::Constant(val) = &expr {
            if *val == 0 {
                let msg = _message.unwrap_or_else(|| "static assertion failed".to_string());
                return Err(format!("_Static_assert failed: {}", msg));
            }
        }
        
        Ok(())
    }
}
