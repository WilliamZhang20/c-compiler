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
            functions.push(self.parse_function()?);
        }
        Ok(Program { functions })
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
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;

        let body_block = self.parse_block()?;

        Ok(Function {
            return_type,
            name,
            body: body_block,
        })
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.advance() {
            Some(Token::Int) => Ok(Type::Int),
            Some(Token::Void) => Ok(Type::Void),
            other => Err(format!("expected type specifier, found {:?}", other)),
        }
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

        if self.check(&|t| matches!(t, Token::OpenBrace)) {
            let block = self.parse_block()?;
            return Ok(Stmt::Block(block));
        }

        let expr = self.parse_expr()?;
        self.expect(|t| matches!(t, Token::Semicolon), "';'")?;
        Ok(Stmt::Expr(expr))
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_equality()?;

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
                _ => Err("invalid assignment target".to_string()),
            }
        } else {
            Ok(left)
        }
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_additive()?;

        while self.match_token(|t| matches!(t, Token::EqualEqual)) {
            let right = self.parse_additive()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::EqualEqual,
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
        if self.match_token(|t| matches!(t, Token::Plus | Token::Minus)) {
            let op = match self.previous().unwrap() {
                Token::Plus => UnaryOp::Plus,
                Token::Minus => UnaryOp::Minus,
                _ => unreachable!(),
            };
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
            })
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Some(Token::Identifier { value }) => Ok(Expr::Variable(value.clone())),
            Some(Token::Constant { value }) => Ok(Expr::Constant(*value)),
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
        // more asserts if you want
    }
}