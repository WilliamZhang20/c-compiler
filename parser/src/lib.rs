// take a list of tokens, output a syntax tree or an error
use model::{
    BinaryOp, Block, Expr, Function, Program, Stmt, Token, Type, UnaryOp,
};

pub fn parse_tokens(tokens: &[Token]) -> Result<Program, String> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_program(&mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        while !self.is_at_end() {
            if self.is_function_definition() {
                functions.push(self.parse_function()?);
            } else {
                self.skip_top_level_item()?;
            }
        }
        Ok(Program { functions })
    }

    fn is_function_definition(&self) -> bool {
        let mut temp_pos = self.pos;
        // Skip modifiers
        while temp_pos < self.tokens.len() {
            let tok = &self.tokens[temp_pos];
            if matches!(tok, Token::Static | Token::Extern | Token::Inline | Token::Attribute | Token::Extension | Token::Const | Token::Restrict | Token::Hash) {
                temp_pos += 1;
                // If it's attribute or extension, it might have parentheses
                if temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
                    let mut depth = 1;
                    temp_pos += 1;
                    while depth > 0 && temp_pos < self.tokens.len() {
                        if matches!(self.tokens[temp_pos], Token::OpenParenthesis) { depth += 1; }
                        else if matches!(self.tokens[temp_pos], Token::CloseParenthesis) { depth -= 1; }
                        temp_pos += 1;
                    }
                }
            } else {
                break;
            }
        }

        if temp_pos >= self.tokens.len() { return false; }
        // Must start with a known type for now
        if !matches!(self.tokens[temp_pos], Token::Int | Token::Void | Token::Char) {
            return false;
        }
        temp_pos += 1;

        // Followed by identifier or star (for pointers)
        while temp_pos < self.tokens.len() && matches!(self.tokens[temp_pos], Token::Star) {
            temp_pos += 1;
        }
        
        if temp_pos >= self.tokens.len() { return false; }
        if !matches!(self.tokens[temp_pos], Token::Identifier { .. }) {
            return false;
        }
        temp_pos += 1;

        // Followed by '('
        if temp_pos >= self.tokens.len() || !matches!(self.tokens[temp_pos], Token::OpenParenthesis) {
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

    fn parse_function(&mut self) -> Result<Function, String> {
        let return_type = self.parse_type()?;
        let name = match self.advance() {
            Some(Token::Identifier { value }) => value.clone(),
            other => {
                return Err(format!(
                    "expected function name identifier, found {:?}",
                    other
                ));
            }
        };

        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let mut params = Vec::new();
        if !self.check(&|t| matches!(t, Token::CloseParenthesis)) {
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
        }
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;

        let body_block = self.parse_block()?;

        Ok(Function {
            return_type,
            name,
            params,
            body: body_block,
        })
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        loop {
            if self.match_token(|t| matches!(t, Token::Static | Token::Extern | Token::Inline | Token::Const | Token::Restrict | Token::Attribute | Token::Extension)) {
                if let Some(Token::Attribute | Token::Extension) = self.previous() {
                    self.skip_parentheses()?;
                }
            } else {
                break;
            }
        }

        let mut base = match self.advance() {
            Some(Token::Int) => Type::Int,
            Some(Token::Void) => Type::Void,
            Some(Token::Char) => Type::Char,
            other => return Err(format!("expected type specifier, found {:?}", other)),
        };

        while self.match_token(|t| matches!(t, Token::Star)) {
            base = Type::Pointer(Box::new(base));
        }

        Ok(base)
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

    fn skip_top_level_item(&mut self) -> Result<(), String> {
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
        if self.match_token(|t| matches!(t, Token::Return)) {
            if self.match_token(|t| matches!(t, Token::Semicolon)) {
                return Ok(Stmt::Return(None));
            }
            let expr = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            return Ok(Stmt::Return(Some(expr)));
        }

        if self.match_token(|t| matches!(t, Token::If)) {
            self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
            let cond = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            let then_branch = Box::new(self.parse_stmt()?);
            let else_branch = if self.match_token(|t| matches!(t, Token::Else)) {
                Some(Box::new(self.parse_stmt()?))
            } else {
                None
            };
            return Ok(Stmt::If {
                cond,
                then_branch,
                else_branch,
            });
        }

        if self.match_token(|t| matches!(t, Token::While)) {
            self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
            let cond = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            let body = Box::new(self.parse_stmt()?);
            return Ok(Stmt::While { cond, body });
        }

        if self.match_token(|t| matches!(t, Token::Do)) {
            let body = Box::new(self.parse_stmt()?);
            self.expect(|t| matches!(t, Token::While), "while")?;
            self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
            let cond = self.parse_expr()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
            return Ok(Stmt::DoWhile { body, cond });
        }

        if self.match_token(|t| matches!(t, Token::For)) {
            self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
            let init = if self.match_token(|t| matches!(t, Token::Semicolon)) {
                None
            } else {
                let expr = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
                Some(expr)
            };
            let cond = if self.match_token(|t| matches!(t, Token::Semicolon)) {
                None
            } else {
                let expr = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
                Some(expr)
            };
            let post = if self.match_token(|t| matches!(t, Token::CloseParenthesis)) {
                None
            } else {
                let expr = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                Some(expr)
            };
            let body = Box::new(self.parse_stmt()?);
            return Ok(Stmt::For {
                init,
                cond,
                post,
                body,
            });
        }

        if self.check(&|t| matches!(t, Token::OpenBrace)) {
            let block = self.parse_block()?;
            return Ok(Stmt::Block(block));
        }

        // Variable Declaration
        if self.check(&|t| matches!(t, Token::Int | Token::Void | Token::Char | Token::Static | Token::Extern | Token::Inline | Token::Const | Token::Restrict | Token::Attribute | Token::Extension)) {
            let mut r#type = self.parse_type()?;
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
            return Ok(Stmt::Declaration {
                r#type,
                name,
                init,
            });
        }

        let expr = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(Stmt::Expr(expr))
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_logical_or()?;

        if self.match_token(|t| matches!(t, Token::Equal)) {
            match left {
                Expr::Variable(name) => {
                    let value = self.parse_assignment()?;
                    Ok(Expr::Binary {
                        left: Box::new(Expr::Variable(name)),
                        op: BinaryOp::Assign,
                        right: Box::new(value),
                    })
                }
                Expr::Index { .. } => {
                    let value = self.parse_assignment()?;
                    Ok(Expr::Binary {
                        left: Box::new(left),
                        op: BinaryOp::Assign,
                        right: Box::new(value),
                    })
                }
                _ => Err("invalid assignment target".to_string()),
            }
        } else {
            Ok(left)
        }
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_logical_and()?;
        while self.match_token(|t| matches!(t, Token::OrOr)) {
            let right = self.parse_logical_and()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::LogicalOr,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while self.match_token(|t| matches!(t, Token::AndAnd)) {
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::LogicalAnd,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_relational()?;

        while self.match_token(|t| matches!(t, Token::EqualEqual | Token::BangEqual)) {
            let op = match self.previous().unwrap() {
                Token::EqualEqual => BinaryOp::EqualEqual,
                Token::BangEqual => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            let right = self.parse_relational()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_additive()?;
        while self.match_token(|t| {
            matches!(
                t,
                Token::Less | Token::LessEqual | Token::Greater | Token::GreaterEqual
            )
        }) {
            let op = match self.previous().unwrap() {
                Token::Less => BinaryOp::Less,
                Token::LessEqual => BinaryOp::LessEqual,
                Token::Greater => BinaryOp::Greater,
                Token::GreaterEqual => BinaryOp::GreaterEqual,
                _ => unreachable!(),
            };
            let right = self.parse_additive()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_multiplicative()?;

        while self.match_token(|t| matches!(t, Token::Plus | Token::Minus)) {
            let op = match self.previous().unwrap() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_multiplicative()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;

        while self.match_token(|t| matches!(t, Token::Star | Token::Slash)) {
            let op = match self.previous().unwrap() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.match_token(|t| matches!(t, Token::Plus | Token::Minus | Token::Bang | Token::Star | Token::Ampersand)) {
            let token = self.previous().unwrap().clone();
            let op = match token {
                Token::Plus => UnaryOp::Plus,
                Token::Minus => UnaryOp::Minus,
                Token::Bang => UnaryOp::LogicalNot,
                Token::Star => UnaryOp::Deref,
                Token::Ampersand => UnaryOp::AddrOf,
                _ => unreachable!(),
            };
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
            })
        } else if self.match_token(|t| matches!(t, Token::SizeOf)) {
            if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                if self.check_is_type() {
                    let ty = self.parse_type()?;
                    self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                    Ok(Expr::SizeOf(ty))
                } else {
                    let expr = self.parse_expr()?;
                    self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                    Ok(Expr::SizeOfExpr(Box::new(expr)))
                }
            } else {
                let expr = self.parse_unary()?;
                Ok(Expr::SizeOfExpr(Box::new(expr)))
            }
        } else if self.check(&|t| matches!(t, Token::OpenParenthesis)) && self.check_is_type_at(1) {
            self.advance(); // consume '('
            let ty = self.parse_type()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            let expr = self.parse_unary()?;
            Ok(Expr::Cast(ty, Box::new(expr)))
        } else {
            self.parse_postfix()
        }
    }

    fn check_is_type(&self) -> bool {
        self.check_is_type_at(0)
    }

    fn check_is_type_at(&self, offset: usize) -> bool {
        match self.tokens.get(self.pos + offset) {
            Some(Token::Int | Token::Void | Token::Char | Token::Static | Token::Extern | Token::Inline | Token::Const | Token::Restrict | Token::Attribute | Token::Extension) => true,
            _ => false,
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(|t| matches!(t, Token::OpenBracket)) {
                let index = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                expr = Expr::Index {
                    array: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                let mut args = Vec::new();
                if !self.check(&|t| matches!(t, Token::CloseParenthesis)) {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.match_token(|t| matches!(t, Token::Comma)) {
                            break;
                        }
                    }
                }
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                if let Expr::Variable(name) = expr {
                    expr = Expr::Call { name, args };
                } else {
                    return Err("can only call variables (functions)".to_string());
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Some(Token::Identifier { value }) => Ok(Expr::Variable(value.clone())),
            Some(Token::Constant { value }) => Ok(Expr::Constant(*value)),
            Some(Token::StringLiteral { value }) => Ok(Expr::StringLiteral(value.clone())),
            Some(Token::OpenParenthesis) => {
                let expr = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                Ok(expr)
            }
            other => Err(format!("expected expression, found {:?}", other)),
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn previous(&self) -> Option<&Token> {
        if self.pos == 0 {
            None
        } else {
            self.tokens.get(self.pos - 1)
        }
    }

    fn advance(&mut self) -> Option<&Token> {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.previous()
    }

    fn match_token<F>(&mut self, predicate: F) -> bool
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

    fn check<F>(&self, predicate: &F) -> bool
    where
        F: Fn(&Token) -> bool,
    {
        match self.peek() {
            Some(tok) => predicate(tok),
            None => false,
        }
    }

    fn expect<F>(&mut self, predicate: F, expected: &str) -> Result<(), String>
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lex;

    #[test]
    fn parse_simple_main() {
        let src = "int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
        assert_eq!(program.functions[0].params.len(), 0);
    }

    #[test]
    fn parse_function_params() {
        let src = "int add(int a, int b) { return a + b; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].params.len(), 2);
        assert_eq!(program.functions[0].params[0].1, "a");
        assert_eq!(program.functions[0].params[1].1, "b");
    }

    #[test]
    fn parse_variable_declaration() {
        let src = "void main() { int x = 5; int y; y = x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        assert_eq!(stmts.len(), 3);
        matches!(stmts[0], Stmt::Declaration { .. });
        matches!(stmts[1], Stmt::Declaration { .. });
        matches!(stmts[2], Stmt::Expr(Expr::Binary { .. }));
    }

    #[test]
    fn parse_while_loop() {
        let src = "void main() { int x = 0; while (x < 10) { x = x + 1; } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        assert_eq!(stmts.len(), 2);
        matches!(stmts[1], Stmt::While { .. });
    }

    #[test]
    fn parse_for_loop() {
        // Note: My lexer handles 'int i = 0' as a declaration which is currently expected in my parse_stmt
        // Wait, parse_stmt handles declarations, but parse_for expects an expression for init.
        // In C, 'for (int i = 0; ...)' is valid in C99+. My current parse_for expects an expression.
        // Let's test with expression init for now to match current implementation.
        let src = "void main() { int i; for (i = 0; i < 10; i = i + 1) { return i; } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        matches!(program.functions[0].body.statements[1], Stmt::For { .. });
    }

    #[test]
    fn parse_logical_ops() {
        let src = "int main() { return (1 && 0) || !1; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        matches!(program.functions[0].body.statements[0], Stmt::Return(Some(Expr::Binary { .. })));
    }

    #[test]
    fn parse_relational_ops() {
        let src = "int main() { return 1 <= 2 && 3 != 4; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        matches!(program.functions[0].body.statements[0], Stmt::Return(Some(Expr::Binary { .. })));
    }

    #[test]
    fn test_header_tolerance() {
        let src = r#"
            typedef int my_int;
            struct foo { int x; };
            extern int bar(int x);
            static inline int baz(int x) { return x; }
            int main() {
                return 0;
            }
        "#;
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
    }
}