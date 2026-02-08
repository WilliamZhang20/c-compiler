use model::{Block, Stmt, Token, Type};
use crate::parser::Parser;
use crate::types::TypeParser;
use crate::expressions::ExpressionParser;

/// Statement parsing functionality
pub(crate) trait StatementParser {
    fn parse_stmt(&mut self) -> Result<Stmt, String>;
    fn parse_block(&mut self) -> Result<Block, String>;
}

impl<'a> StatementParser for Parser<'a> {
    fn parse_block(&mut self) -> Result<Block, String> {
        self.expect(|t| matches!(t, Token::OpenBrace), "'{'")?;
        let mut statements = Vec::new();
        while !self.check(&|t| matches!(t, Token::CloseBrace)) && !self.is_at_end() {
            statements.push(self.parse_stmt()?);
        }
        self.expect(|t| matches!(t, Token::CloseBrace), "'}'")?;
        Ok(Block { statements })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
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
        if self.check(&|t| matches!(t, Token::OpenBrace)) {
            let block = self.parse_block()?;
            return Ok(Stmt::Block(block));
        }

        // Variable declaration
        if self.check_is_type() {
            return self.parse_declaration();
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
        let mut r#type = self.parse_type()?;

        // Check for function pointer: type (*name)(params)
        if self.check(&|t| matches!(t, Token::OpenParenthesis)) {
            // Could be function pointer or just grouped expression
            // Peek ahead to see if it's (*identifier)
            let saved_pos = self.pos;
            self.advance(); // consume (

            if self.match_token(|t| matches!(t, Token::Star)) {
                // It's a function pointer
                let name = match self.advance() {
                    Some(Token::Identifier { value }) => value.clone(),
                    other => {
                        return Err(format!(
                            "expected identifier after '(*' in function pointer, found {:?}",
                            other
                        ))
                    }
                };
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;

                // Parse parameter types
                let mut param_types = Vec::new();
                if !self.check(&|t| matches!(t, Token::CloseParenthesis)) {
                    loop {
                        let param_type = self.parse_type()?;
                        param_types.push(param_type);
                        // Optional parameter name
                        if self.check(&|t| matches!(t, Token::Identifier { .. })) {
                            self.advance();
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
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

                return Ok(Stmt::Declaration {
                    r#type,
                    name,
                    init,
                });
            } else {
                // Not a function pointer, restore position
                self.pos = saved_pos;
            }
        }

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
            r#type = Type::Array(Box::new(r#type), size);
        }

        let init = if self.match_token(|t| matches!(t, Token::Equal)) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;

        Ok(Stmt::Declaration {
            r#type,
            name,
            init,
        })
    }
}
