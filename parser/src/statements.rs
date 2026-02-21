use model::{Block, Expr, InitItem, Designator, Stmt, Token, Type};
use crate::parser::Parser;
use crate::types::TypeParser;
use crate::expressions::ExpressionParser;
use crate::declarations::DeclarationParser;
use crate::utils::ParserUtils;

/// Statement parsing functionality
pub(crate) trait StatementParser {
    fn parse_stmt(&mut self) -> Result<Stmt, String>;
    fn parse_block(&mut self) -> Result<Block, String>;
}

impl<'a> StatementParser for Parser<'a> {
    fn parse_block(&mut self) -> Result<Block, String> {
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;
        let mut statements = Vec::new();
        while !self.check(|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            statements.push(self.parse_stmt()?);
        }
        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        Ok(Block { statements })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        // Empty statement: a lone semicolon
        if self.match_token(|t| matches!(t, Token::Semicolon)) {
            return Ok(Stmt::Block(Block { statements: vec![] }));
        }

        // Return statement
        if self.match_token(|t| matches!(t, Token::Return)) {
            return self.parse_return_stmt();
        }

        // Break statement
        if self.match_token(|t| matches!(t, Token::Break)) {
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            return Ok(Stmt::Break);
        }

        // Continue statement
        if self.match_token(|t| matches!(t, Token::Continue)) {
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            return Ok(Stmt::Continue);
        }

        // Goto statement
        if self.match_token(|t| matches!(t, Token::Goto)) {
            let label = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected label name after 'goto', found {:?}", other)),
            };
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            return Ok(Stmt::Goto(label));
        }

        // Inline assembly
        if self.match_token(|t| matches!(t, Token::Asm)) {
            return self.parse_inline_asm();
        }

        // Control flow statements
        if self.match_token(|t| matches!(t, Token::If)) {
            return self.parse_if_stmt();
        }

        if self.match_token(|t| matches!(t, Token::While)) {
            return self.parse_while_stmt();
        }

        if self.match_token(|t| matches!(t, Token::Do)) {
            return self.parse_do_while_stmt();
        }

        if self.match_token(|t| matches!(t, Token::For)) {
            return self.parse_for_stmt();
        }

        if self.match_token(|t| matches!(t, Token::Switch)) {
            return self.parse_switch_stmt();
        }

        if self.match_token(|t| matches!(t, Token::Case)) {
            return self.parse_case_stmt();
        }

        if self.match_token(|t| matches!(t, Token::Default)) {
            self.expect(|t| matches!(t, Token::Colon), "':'")?;
            return Ok(Stmt::Default);
        }

        // Block statement
        if self.check(|t| matches!(t, Token::OpenBrace)) {
            let block = self.parse_block()?;
            return Ok(Stmt::Block(block));
        }

        // _Static_assert
        if self.match_token(|t| matches!(t, Token::StaticAssert)) {
            self.parse_static_assert()?;
            return Ok(Stmt::Block(Block { statements: vec![] })); // No-op statement
        }

        // Variable declaration
        if self.check_is_type() {
            return self.parse_declaration();
        }

        // Check for label (identifier followed by colon)
        // We need to lookahead to distinguish from expression statements
        if self.check(|t| matches!(t, Token::Identifier { .. })) {
            let saved_pos = self.pos;
            let label_name = if let Some(Token::Identifier { value }) = self.advance() {
                value.clone()
            } else {
                String::new()
            };
            
            if !label_name.is_empty() && self.check(|t| matches!(t, Token::Colon)) {
                // It's a label
                self.advance(); // consume colon
                return Ok(Stmt::Label(label_name));
            }
            // Not a label, restore position
            self.pos = saved_pos;
        }

        // Expression statement
        let expr = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(Stmt::Expr(expr))
    }
}

impl<'a> Parser<'a> {
    fn parse_return_stmt(&mut self) -> Result<Stmt, String> {
        if self.match_token(|t| matches!(t, Token::Semicolon)) {
            return Ok(Stmt::Return(None));
        }
        let expr = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(Stmt::Return(Some(expr)))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let cond = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        let then_branch = Box::new(self.parse_stmt()?);
        let else_branch = if self.match_token(|t| matches!(t, Token::Else)) {
            Some(Box::new(self.parse_stmt()?))
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let cond = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::While { cond, body })
    }

    fn parse_do_while_stmt(&mut self) -> Result<Stmt, String> {
        let body = Box::new(self.parse_stmt()?);
        self.expect(|t| matches!(t, Token::While), "while")?;
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let cond = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(Stmt::DoWhile { body, cond })
    }

    fn parse_for_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;

        // Init clause
        let init = if self.match_token(|t| matches!(t, Token::Semicolon)) {
            None
        } else {
            // Parse a statement (declaration or expression)
            Some(Box::new(self.parse_stmt()?))
        };

        // Condition clause
        let cond = if self.match_token(|t| matches!(t, Token::Semicolon)) {
            None
        } else {
            let expr = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            Some(expr)
        };

        // Post clause
        let post = if self.match_token(|t| matches!(t, Token::CloseParenthesis)) {
            None
        } else {
            let expr = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            Some(expr)
        };

        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::For {
            init,
            cond,
            post,
            body,
        })
    }

    fn parse_switch_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let cond = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::Switch { cond, body })
    }

    fn parse_case_stmt(&mut self) -> Result<Stmt, String> {
        let expr = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::Colon), "':'")?;
        Ok(Stmt::Case(expr))
    }

    fn parse_declaration(&mut self) -> Result<Stmt, String> {
        let (mut r#type, qualifiers) = self.parse_type_with_qualifiers()?;

        // Check for function pointer: type (*name)(params)
        if self.check(|t| matches!(t, Token::OpenParenthesis)) {
            // Could be function pointer or just grouped expression
            // Peek ahead to see if it's (*identifier)
            let saved_pos = self.pos;
            self.advance(); // consume (

            if self.match_token(|t| matches!(t, Token::Star)) {
                // It's a function pointer
                let name = match self.advance() {
                    Some(Token::Identifier { value }) => value.clone(),
                    _other => {
                        // Can't parse this function pointer, bail out
                        return Err("Cannot parse function pointer declaration".to_string());
                    }
                };
                
                if !self.match_token(|t| matches!(t, Token::CloseParenthesis)) {
                    // Malformed function pointer
                    return Err("Expected ')' after function pointer name".to_string());
                }
                
                if !self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                    // Malformed function pointer
                    return Err("Expected '(' for function pointer parameters".to_string());
                }

                // Parse parameter types
                let mut param_types = Vec::new();
                if !self.check(|t| matches!(t, Token::CloseParenthesis)) {
                    loop {
                        let param_type = self.parse_type()?;
                        param_types.push(param_type.clone());
                        // Optional parameter name
                        if self.check(|t| matches!(t, Token::Identifier { .. })) {
                            self.advance();
                        }
                        // Handle array syntax: type name[] or type[] (supports multi-dimensional)
                        while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                            // Check if array size is provided
                            let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                                0 // Use 0 to represent unsized array
                            } else {
                                match self.advance() {
                                    Some(Token::Constant { value }) => *value as usize,
                                    other => return Err(format!("expected constant array size in function pointer parameter, found {:?}", other)),
                                }
                            };
                            self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                            // Update the last parameter type to be an array
                            if let Some(last_param) = param_types.last_mut() {
                                let inner = last_param.clone();
                                *last_param = Type::Array(Box::new(inner), size);
                            }
                        }
                        if !self.match_token(|t| matches!(t, Token::Comma)) {
                            break;
                        }
                    }
                }
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;

                r#type = Type::FunctionPointer {
                    return_type: Box::new(r#type),
                    param_types,
                };

                let init = if self.match_token(|t| matches!(t, Token::Equal)) {
                    Some(self.parse_assignment()?)
                } else {
                    None
                };
                self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

                return Ok(Stmt::Declaration {
                    r#type,
                    qualifiers: qualifiers.clone(),
                    name,
                    init,
                });
            } else {
                // Not a function pointer, restore position
                self.pos = saved_pos;
            }
        }

        // base_type holds the type parsed so far (before any per-declarator array dims).
        // We use it to reset for each declarator in a comma-separated list, e.g.
        //   int a = 1, b = 2, c;
        //   int arr[3], x;
        let base_type = r#type;
        let mut declarations: Vec<Stmt> = Vec::new();

        loop {
            let mut decl_type = base_type.clone();

            let name = match self.advance() {
                Some(Token::Identifier { value }) => value.clone(),
                other => return Err(format!("expected identifier after type, found {:?}", other)),
            };

            // Check for array dimensions on this declarator (supports multi-dimensional)
            while self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Check if array size is provided (empty brackets [] are allowed)
                let size = if self.check(|t| matches!(t, Token::CloseBracket)) {
                    0 // Use 0 to represent unsized array
                } else {
                    match self.advance() {
                        Some(Token::Constant { value }) => *value as usize,
                        other => return Err(format!("[parse_declaration] expected constant array size, found {:?}", other)),
                    }
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                decl_type = Type::Array(Box::new(decl_type), size);
            }

            let init = if self.match_token(|t| matches!(t, Token::Equal)) {
                if self.check(|t| matches!(t, Token::OpenBrace)) {
                    Some(self.parse_init_list()?)
                } else {
                    // Use parse_assignment so commas act as multi-decl
                    // separators, not comma operators.
                    Some(self.parse_assignment()?)
                }
            } else {
                None
            };

            // Infer array size from initializer
            if let Type::Array(inner, 0) = &decl_type {
                if let Some(Expr::StringLiteral(s)) = &init {
                    decl_type = Type::Array(inner.clone(), s.len() + 1);
                } else if let Some(Expr::InitList(items)) = &init {
                    decl_type = Type::Array(inner.clone(), items.len());
                }
            }

            declarations.push(Stmt::Declaration {
                r#type: decl_type,
                qualifiers: qualifiers.clone(),
                name,
                init,
            });

            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
        }

        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

        if declarations.len() == 1 {
            Ok(declarations.remove(0))
        } else {
            Ok(Stmt::MultiDecl(declarations))
        }
    }

    fn parse_inline_asm(&mut self) -> Result<Stmt, String> {
        // asm [volatile] ( "assembly template" : outputs : inputs : clobbers );
        let is_volatile = self.match_token(|t| matches!(t, Token::Volatile));
        
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        
        // Parse assembly template string
        let template = match self.advance() {
            Some(Token::StringLiteral { value }) => value.clone(),
            other => return Err(format!("expected string literal for asm template, found {:?}", other)),
        };
        
        // Check for operands and clobbers
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut clobbers = Vec::new();
        
        // Parse outputs (if present)
        if self.match_token(|t| matches!(t, Token::Colon)) {
            if !self.check(|t| matches!(t, Token::Colon | Token::CloseParenthesis)) {
                loop {
                    let constraint = match self.advance() {
                        Some(Token::StringLiteral { value }) => value.clone(),
                        other => return Err(format!("expected constraint string, found {:?}", other)),
                    };
                    self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                    let expr = self.parse_expr()?;
                    self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                    outputs.push(model::AsmOperand { constraint, expr });
                    
                    if !self.match_token(|t| matches!(t, Token::Comma)) {
                        break;
                    }
                }
            }
            
            // Parse inputs (if present)
            if self.match_token(|t| matches!(t, Token::Colon)) {
                if !self.check(|t| matches!(t, Token::Colon | Token::CloseParenthesis)) {
                    loop {
                        let constraint = match self.advance() {
                            Some(Token::StringLiteral { value }) => value.clone(),
                            other => return Err(format!("expected constraint string, found {:?}", other)),
                        };
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let expr = self.parse_expr()?;
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        inputs.push(model::AsmOperand { constraint, expr });
                        
                        if !self.match_token(|t| matches!(t, Token::Comma)) {
                            break;
                        }
                    }
                }
                
                // Parse clobbers (if present)
                if self.match_token(|t| matches!(t, Token::Colon)) {
                    if !self.check(|t| matches!(t, Token::CloseParenthesis)) {
                        loop {
                            let clobber = match self.advance() {
                                Some(Token::StringLiteral { value }) => value.clone(),
                                other => return Err(format!("expected clobber string, found {:?}", other)),
                            };
                            clobbers.push(clobber);
                            
                            if !self.match_token(|t| matches!(t, Token::Comma)) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        
        Ok(Stmt::InlineAsm {
            template,
            outputs,
            inputs,
            clobbers,
            is_volatile,
        })
    }

    /// Parse a brace-enclosed initializer list: `{ expr, expr, ... }`
    /// Supports designated initializers: `{ .field = expr, [idx] = expr }`
    /// and nested initializer lists: `{ {1,2}, {3,4} }`
    pub(crate) fn parse_init_list(&mut self) -> Result<Expr, String> {
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;
        let mut items = Vec::new();

        // Handle empty initializer list `{}`
        if self.match_token(|t| matches!(t, Token::CloseBrace)) {
            return Ok(Expr::InitList(items));
        }

        loop {
            let designator = if self.match_token(|t| matches!(t, Token::Dot)) {
                // Designated field: .field = expr
                let field_name = match self.advance() {
                    Some(Token::Identifier { value }) => value.clone(),
                    other => return Err(format!("expected field name after '.', found {:?}", other)),
                };
                self.expect(|t| matches!(t, Token::Equal), "'='")?;
                Some(Designator::Field(field_name))
            } else if self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Designated index: [index] = expr
                let index = match self.advance() {
                    Some(Token::Constant { value }) => *value,
                    other => return Err(format!("expected constant index in designator, found {:?}", other)),
                };
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                self.expect(|t| matches!(t, Token::Equal), "'='")?;
                Some(Designator::Index(index))
            } else {
                None
            };

            // Parse the value — may be a nested init list or an assignment expr
            let value = if self.check(|t| matches!(t, Token::OpenBrace)) {
                self.parse_init_list()?
            } else {
                self.parse_assignment()?
            };

            items.push(InitItem { designator, value });

            if !self.match_token(|t| matches!(t, Token::Comma)) {
                break;
            }
            // Allow trailing comma before closing brace
            if self.check(|t| matches!(t, Token::CloseBrace)) {
                break;
            }
        }

        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        Ok(Expr::InitList(items))
    }
}
