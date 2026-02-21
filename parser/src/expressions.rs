use model::{BinaryOp, Expr, Token, Type, UnaryOp};
use crate::parser::Parser;
use crate::types::TypeParser;
use crate::statements::StatementParser;
use crate::utils::ParserUtils;

/// Expression parsing functionality using precedence climbing
pub(crate) trait ExpressionParser {
    fn parse_expr(&mut self) -> Result<Expr, String>;
    /// Parse a constant expression and evaluate it to a usize (for array sizes)
    fn parse_array_size(&mut self) -> Result<usize, String>;
}

impl<'a> ExpressionParser for Parser<'a> {
    fn parse_expr(&mut self) -> Result<Expr, String> {
        let first = self.parse_assignment()?;
        if self.check(|t| matches!(t, Token::Comma)) {
            let mut exprs = vec![first];
            while self.match_token(|t| matches!(t, Token::Comma)) {
                exprs.push(self.parse_assignment()?);
            }
            Ok(Expr::Comma(exprs))
        } else {
            Ok(first)
        }
    }
    
    fn parse_array_size(&mut self) -> Result<usize, String> {
        let expr = self.parse_conditional()?;
        const_eval_expr(&expr)
            .map(|v| v as usize)
            .ok_or_else(|| format!("expected constant array size expression, got {:?}", expr))
    }
}

/// Evaluate a constant expression at compile time (for array sizes, etc.)
fn const_eval_expr(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Constant(v) => Some(*v),
        Expr::SizeOf(ty) => Some(const_sizeof(ty)),
        Expr::AlignOf(ty) => Some(const_alignof(ty)),
        Expr::Cast(_, inner) => const_eval_expr(inner),
        Expr::Unary { op, expr } => {
            let v = const_eval_expr(expr)?;
            match op {
                UnaryOp::Minus => Some(-v),
                UnaryOp::BitwiseNot => Some(!v),
                UnaryOp::LogicalNot => Some(if v == 0 { 1 } else { 0 }),
                _ => None,
            }
        }
        Expr::Binary { left, op, right } => {
            let l = const_eval_expr(left)?;
            let r = const_eval_expr(right)?;
            match op {
                BinaryOp::Add => Some(l + r),
                BinaryOp::Sub => Some(l - r),
                BinaryOp::Mul => Some(l * r),
                BinaryOp::Div => if r != 0 { Some(l / r) } else { None },
                BinaryOp::Mod => if r != 0 { Some(l % r) } else { None },
                BinaryOp::ShiftLeft => if r >= 0 && r < 64 { Some(l << r) } else { None },
                BinaryOp::ShiftRight => if r >= 0 && r < 64 { Some(l >> r) } else { None },
                BinaryOp::BitwiseAnd => Some(l & r),
                BinaryOp::BitwiseOr => Some(l | r),
                BinaryOp::BitwiseXor => Some(l ^ r),
                BinaryOp::Less => Some(if l < r { 1 } else { 0 }),
                BinaryOp::LessEqual => Some(if l <= r { 1 } else { 0 }),
                BinaryOp::Greater => Some(if l > r { 1 } else { 0 }),
                BinaryOp::GreaterEqual => Some(if l >= r { 1 } else { 0 }),
                BinaryOp::EqualEqual => Some(if l == r { 1 } else { 0 }),
                BinaryOp::NotEqual => Some(if l != r { 1 } else { 0 }),
                _ => None,
            }
        }
        Expr::Conditional { condition, then_expr, else_expr } => {
            let cond = const_eval_expr(condition)?;
            if cond != 0 {
                const_eval_expr(then_expr)
            } else {
                const_eval_expr(else_expr)
            }
        }
        _ => None,
    }
}

/// Compile-time sizeof for common types (used in constant expressions)
fn const_sizeof(ty: &Type) -> i64 {
    match ty {
        Type::Char | Type::UnsignedChar | Type::Bool => 1,
        Type::Short | Type::UnsignedShort => 2,
        Type::Int | Type::UnsignedInt | Type::Float => 4,
        Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong
            | Type::Double | Type::Pointer(_) | Type::FunctionPointer { .. } => 8,
        Type::Array(inner, n) => const_sizeof(inner) * (*n as i64),
        Type::Void => 1, // GCC extension
        _ => 4, // Default fallback
    }
}

/// Compile-time alignof for common types
fn const_alignof(ty: &Type) -> i64 {
    match ty {
        Type::Char | Type::UnsignedChar | Type::Bool => 1,
        Type::Short | Type::UnsignedShort => 2,
        Type::Int | Type::UnsignedInt | Type::Float => 4,
        Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong
            | Type::Double | Type::Pointer(_) | Type::FunctionPointer { .. } => 8,
        _ => 4,
    }
}

impl<'a> Parser<'a> {
    // Assignment (lowest precedence)
    pub(crate) fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_conditional()?;

        if self.check(|t| matches!(t, Token::Equal 
            | Token::PlusEqual | Token::MinusEqual | Token::StarEqual | Token::SlashEqual 
            | Token::PercentEqual | Token::AndEqual | Token::OrEqual | Token::XorEqual 
            | Token::LessLessEqual | Token::GreaterGreaterEqual)) 
        {
            let token = self.advance().unwrap().clone();
            let op = match token {
                Token::Equal => BinaryOp::Assign,
                Token::PlusEqual => BinaryOp::AddAssign,
                Token::MinusEqual => BinaryOp::SubAssign,
                Token::StarEqual => BinaryOp::MulAssign,
                Token::SlashEqual => BinaryOp::DivAssign,
                Token::PercentEqual => BinaryOp::ModAssign,
                Token::AndEqual => BinaryOp::BitwiseAndAssign,
                Token::OrEqual => BinaryOp::BitwiseOrAssign,
                Token::XorEqual => BinaryOp::BitwiseXorAssign,
                Token::LessLessEqual => BinaryOp::ShiftLeftAssign,
                Token::GreaterGreaterEqual => BinaryOp::ShiftRightAssign,
                _ => unreachable!(),
            };

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
                        op,
                        right: Box::new(right),
                    })
                }
                _ => Err(format!("invalid assignment target: {:?}", left)),
            }
        } else {
            Ok(left)
        }
    }

    // Conditional/Ternary (? :) operator
    pub(crate) fn parse_conditional(&mut self) -> Result<Expr, String> {
        let condition = self.parse_logical_or()?;

        if self.match_token(|t| matches!(t, Token::Question)) {
            // GNU extension: `a ?: b` — omitted middle operand
            // means `a ? a : b` (condition evaluated only once)
            let then_expr = if self.check(|t| matches!(t, Token::Colon)) {
                // Omitted middle: reuse the condition expression
                condition.clone()
            } else {
                self.parse_expr()?
            };
            self.expect(|t| matches!(t, Token::Colon), "':' in conditional expression")?;
            let else_expr = self.parse_conditional()?;
            Ok(Expr::Conditional {
                condition: Box::new(condition),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            Ok(condition)
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
        } else if self.match_token(|t| matches!(t, Token::PlusPlus)) {
            // Prefix increment
            let expr = self.parse_unary()?;
            Ok(Expr::PrefixIncrement(Box::new(expr)))
        } else if self.match_token(|t| matches!(t, Token::MinusMinus)) {
            // Prefix decrement
            let expr = self.parse_unary()?;
            Ok(Expr::PrefixDecrement(Box::new(expr)))
        } else if self.match_token(|t| matches!(t, Token::SizeOf)) {
            self.parse_sizeof()
        } else if self.match_token(|t| matches!(t, Token::AlignOf)) {
            self.parse_alignof()
        } else if self.check(|t| matches!(t, Token::OpenParenthesis)) && self.check_is_type_at(1)
        {
            // Cast or compound literal: (type)expr  or  (type){init}
            self.advance(); // consume '('
            let ty = self.parse_type()?;
            self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
            if self.check(|t| matches!(t, Token::OpenBrace)) {
                // Compound literal: (type){init_list}
                let init_expr = self.parse_init_list()?;
                let items = match init_expr {
                    Expr::InitList(items) => items,
                    _ => unreachable!(),
                };
                // Compound literals can appear in postfix position
                // (e.g., (struct foo){...}.member), so wrap via parse_postfix_on
                let lit = Expr::CompoundLiteral { r#type: ty, init: items };
                Ok(lit)
            } else {
                let expr = self.parse_unary()?;
                Ok(Expr::Cast(ty, Box::new(expr)))
            }
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

    fn parse_alignof(&mut self) -> Result<Expr, String> {
        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
        let ty = self.parse_type()?;
        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
        Ok(Expr::AlignOf(ty))
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
                // Function call – each argument is an assignment-expression,
                // NOT an expression (which would greedily eat commas).
                let mut args = Vec::new();
                if !self.check(|t| matches!(t, Token::CloseParenthesis)) {
                    loop {
                        args.push(self.parse_assignment()?);
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
            } else if self.match_token(|t| matches!(t, Token::PlusPlus)) {
                // Postfix increment
                expr = Expr::PostfixIncrement(Box::new(expr));
            } else if self.match_token(|t| matches!(t, Token::MinusMinus)) {
                // Postfix decrement
                expr = Expr::PostfixDecrement(Box::new(expr));
            } else {
                break;
            }
        }

        Ok(expr)
    }

    // Primary (literals, identifiers, parenthesized expressions)
    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Some(Token::Identifier { value }) => {
                // Handle GCC builtins at parse time
                match value.as_str() {
                    "__builtin_expect" | "__builtin_expect_with_probability" => {
                        // __builtin_expect(expr, expected_val) → expr
                        // Used by likely()/unlikely() macros in the kernel.
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let expr = self.parse_assignment()?;
                        self.expect(|t| matches!(t, Token::Comma), "','")?;
                        // Consume and discard the expected value (and any extra args)
                        let _ = self.parse_assignment()?;
                        // Handle __builtin_expect_with_probability extra arg
                        if self.check(|t| matches!(t, Token::Comma)) {
                            self.advance();
                            let _ = self.parse_assignment()?;
                        }
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        Ok(expr)
                    }
                    "__builtin_constant_p" => {
                        // __builtin_constant_p(expr) → 0 at runtime
                        // (we're a simple compiler, nothing is provably constant)
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let _ = self.parse_assignment()?;
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        Ok(Expr::Constant(0))
                    }
                    "__builtin_offsetof" => {
                        // __builtin_offsetof(type, member) → constant offset
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let ty = self.parse_type()?;
                        self.expect(|t| matches!(t, Token::Comma), "','")?;
                        let member = match self.advance() {
                            Some(Token::Identifier { value }) => value.clone(),
                            other => return Err(format!("expected member name in __builtin_offsetof, found {:?}", other)),
                        };
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        Ok(Expr::BuiltinOffsetof { r#type: ty, member })
                    }
                    "__builtin_types_compatible_p" => {
                        // __builtin_types_compatible_p(type1, type2) → 1 if compatible, 0 otherwise
                        // Used in kernel's __same_type() macro
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let type1 = self.parse_type()?;
                        self.expect(|t| matches!(t, Token::Comma), "','")?;
                        let type2 = self.parse_type()?;
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        let compatible = if type1 == type2 { 1 } else { 0 };
                        Ok(Expr::Constant(compatible))
                    }
                    "__builtin_choose_expr" => {
                        // __builtin_choose_expr(const_expr, expr1, expr2)
                        // → expr1 if const_expr is nonzero, expr2 otherwise
                        self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                        let cond = self.parse_assignment()?;
                        self.expect(|t| matches!(t, Token::Comma), "','")?;
                        let expr1 = self.parse_assignment()?;
                        self.expect(|t| matches!(t, Token::Comma), "','")?;
                        let expr2 = self.parse_assignment()?;
                        self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                        // Evaluate at compile time
                        match cond {
                            Expr::Constant(v) if v != 0 => Ok(expr1),
                            Expr::Constant(_) => Ok(expr2),
                            _ => Ok(expr1), // Default to first if not constant
                        }
                    }
                    _ => Ok(Expr::Variable(value.clone())),
                }
            }
            Some(Token::Constant { value }) => Ok(Expr::Constant(*value)),
            Some(Token::FloatLiteral { value }) => Ok(Expr::FloatConstant(*value)),
            Some(Token::StringLiteral { value }) => Ok(Expr::StringLiteral(value.clone())),
            Some(Token::OpenParenthesis) => {
                // Check for statement expression: ({ ... })
                if self.check(|t| matches!(t, Token::OpenBrace)) {
                    // GNU statement expression
                    let block = self.parse_block()?;
                    self.expect(|t| matches!(t, Token::CloseParenthesis), "')' after statement expression")?;
                    return Ok(Expr::StmtExpr(block.statements));
                }
                let expr = self.parse_expr()?;
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                Ok(expr)
            }
            Some(Token::Generic) => {
                // _Generic(controlling_expr, type: expr, type: expr, ..., default: expr)
                self.expect(|t| matches!(t, Token::OpenParenthesis), "'('")?;
                let ctrl_expr = self.parse_assignment()?;
                self.expect(|t| matches!(t, Token::Comma), "','")?;
                
                let mut associations: Vec<(Option<Type>, Expr)> = Vec::new();
                
                loop {
                    if self.check(|t| matches!(t, Token::CloseParenthesis)) {
                        break;
                    }
                    
                    if self.check(|t| matches!(t, Token::Default)) {
                        self.advance();
                        self.expect(|t| matches!(t, Token::Colon), "':'")?;
                        let expr = self.parse_assignment()?;
                        associations.push((None, expr));
                    } else if self.check_is_type() {
                        let ty = self.parse_type()?;
                        self.expect(|t| matches!(t, Token::Colon), "':'")?;
                        let expr = self.parse_assignment()?;
                        associations.push((Some(ty), expr));
                    } else {
                        self.advance();
                    }
                    
                    if !self.match_token(|t| matches!(t, Token::Comma)) {
                        break;
                    }
                }
                
                self.expect(|t| matches!(t, Token::CloseParenthesis), "')'")?;
                
                Ok(Expr::Generic {
                    controlling: Box::new(ctrl_expr),
                    associations,
                })
            }
            other => Err(format!("expected expression, found {:?}", other)),
        }
    }
}
