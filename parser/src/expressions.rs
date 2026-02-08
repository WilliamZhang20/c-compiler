use model::{BinaryOp, Expr, Token, UnaryOp};
use crate::parser::Parser;
use crate::types::TypeParser;

/// Expression parsing functionality using precedence climbing
pub(crate) trait ExpressionParser {
    fn parse_expr(&mut self) -> Result<Expr, String>;
}

impl<'a> ExpressionParser for Parser<'a> {
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }
}

impl<'a> Parser<'a> {
    // Assignment (lowest precedence)
    pub(crate) fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_logical_or()?;

        if self.match_token(|t| matches!(t, Token::Equal)) {
            match left {
                Expr::Variable(_)
                | Expr::Index { .. }
                | Expr::Member { .. }
                | Expr::PtrMember { .. }
                | Expr::Unary {
                    op: UnaryOp::Deref,
                    ..
                } => {
                    let right = self.parse_assignment()?;
                    Ok(Expr::Binary {
                        left: Box::new(left),
                        op: BinaryOp::Assign,
                        right: Box::new(right),
                    })
                }
                _ => Err(format!("invalid assignment target: {:?}", left)),
            }
        } else {
            Ok(left)
        }
    }

    // Logical OR
    pub(crate) fn parse_logical_or(&mut self) -> Result<Expr, String> {
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

    // Logical AND
    pub(crate) fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_or()?;
        while self.match_token(|t| matches!(t, Token::AndAnd)) {
            let right = self.parse_bitwise_or()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::LogicalAnd,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Bitwise OR
    pub(crate) fn parse_bitwise_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_xor()?;
        while self.match_token(|t| matches!(t, Token::Pipe)) {
            let right = self.parse_bitwise_xor()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseOr,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Bitwise XOR
    pub(crate) fn parse_bitwise_xor(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_and()?;
        while self.match_token(|t| matches!(t, Token::Caret)) {
            let right = self.parse_bitwise_and()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseXor,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Bitwise AND
    pub(crate) fn parse_bitwise_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while self.match_token(|t| matches!(t, Token::Ampersand)) {
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseAnd,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Equality (== !=)
    pub(crate) fn parse_equality(&mut self) -> Result<Expr, String> {
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

    // Relational (< <= > >=)
    pub(crate) fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_shift()?;
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
            let right = self.parse_shift()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    // Shift (<< >>)
    pub(crate) fn parse_shift(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_additive()?;
        while self.match_token(|t| matches!(t, Token::LessLess | Token::GreaterGreater)) {
            let op = match self.previous().unwrap() {
                Token::LessLess => BinaryOp::ShiftLeft,
                Token::GreaterGreater => BinaryOp::ShiftRight,
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

    // Additive (+ -)
    pub(crate) fn parse_additive(&mut self) -> Result<Expr, String> {
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

    // Multiplicative (* / %)
    pub(crate) fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;

        while self.match_token(|t| matches!(t, Token::Star | Token::Slash | Token::Percent)) {
            let op = match self.previous().unwrap() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
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

    // Unary (+ - ! ~ * & sizeof cast)
    pub(crate) fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.match_token(|t| {
            matches!(
                t,
                Token::Plus
                    | Token::Minus
                    | Token::Bang
                    | Token::Tilde
                    | Token::Star
                    | Token::Ampersand
            )
        }) {
            let token = self.previous().unwrap().clone();
            let op = match token {
                Token::Plus => UnaryOp::Plus,
                Token::Minus => UnaryOp::Minus,
                Token::Bang => UnaryOp::LogicalNot,
                Token::Tilde => UnaryOp::BitwiseNot,
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
            self.parse_sizeof()
        } else if self.check(&|t| matches!(t, Token::OpenParenthesis)) && self.check_is_type_at(1)
        {
            // Cast: (type)expr
            self.advance(); // consume '('
            let ty = self.parse_type()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            let expr = self.parse_unary()?;
            Ok(Expr::Cast(ty, Box::new(expr)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_sizeof(&mut self) -> Result<Expr, String> {
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
    }

    // Postfix ([] () . ->)
    pub(crate) fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(|t| matches!(t, Token::OpenBracket)) {
                // Array subscript
                let index = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::CloseBracket), "']'")?;
                expr = Expr::Index {
                    array: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.match_token(|t| matches!(t, Token::OpenParenthesis)) {
                // Function call
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
                expr = Expr::Call {
                    func: Box::new(expr),
                    args,
                };
            } else if self.match_token(|t| matches!(t, Token::Dot)) {
                // Struct member access
                let member = match self.advance() {
                    Some(Token::Identifier { value }) => value.clone(),
                    other => return Err(format!("expected member name after '.', found {:?}", other)),
                };
                expr = Expr::Member {
                    expr: Box::new(expr),
                    member,
                };
            } else if self.match_token(|t| matches!(t, Token::Arrow)) {
                // Pointer member access
                let member = match self.advance() {
                    Some(Token::Identifier { value }) => value.clone(),
                    other => {
                        return Err(format!("expected member name after '->', found {:?}", other))
                    }
                };
                expr = Expr::PtrMember {
                    expr: Box::new(expr),
                    member,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    // Primary (literals, identifiers, parenthesized expressions)
    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
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
}
