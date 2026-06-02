//! Shared C type inference and compatibility rules (C11 §6.3).
//!
//! Used by the semantic analyzer for validation; mirrors rules applied during IR lowering.

use crate::{
    BinaryOp, Expr, FunctionPrototype, Program, StructDef, StructField, Type, TypeQualifiers, UnaryOp,
    UnionDef,
};
use std::collections::{HashMap, HashSet};

/// Resolved function signature for call checking.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSig {
    pub return_type: Type,
    pub param_types: Vec<Type>,
    pub is_variadic: bool,
}

/// Environment for type inference and compatibility checks.
#[derive(Debug, Clone)]
pub struct TypeEnv {
    pub globals: HashMap<String, Type>,
    pub functions: HashMap<String, FunctionSig>,
    pub typedefs: HashMap<String, Type>,
    pub structs: HashMap<String, StructDef>,
    pub unions: HashMap<String, UnionDef>,
    pub forward_structs: HashSet<String>,
    pub enum_constants: HashSet<String>,
}

impl TypeEnv {
    pub fn from_program(program: &Program) -> Self {
        let mut env = Self {
            globals: HashMap::new(),
            functions: HashMap::new(),
            typedefs: program.typedefs.clone(),
            structs: program
                .structs
                .iter()
                .map(|s| (s.name.clone(), s.clone()))
                .collect(),
            unions: program
                .unions
                .iter()
                .map(|u| (u.name.clone(), u.clone()))
                .collect(),
            forward_structs: program.forward_structs.iter().cloned().collect(),
            enum_constants: HashSet::new(),
        };

        for e in &program.enums {
            for (name, _) in &e.constants {
                env.enum_constants.insert(name.clone());
            }
        }

        for g in &program.globals {
            let resolved = env.resolve_type(&g.r#type);
            env.globals.entry(g.name.clone()).or_insert(resolved);
        }

        for proto in &program.prototypes {
            env.register_function(proto);
        }
        for f in &program.functions {
            env.functions.insert(
                f.name.clone(),
                FunctionSig {
                    return_type: env.resolve_type(&f.return_type),
                    param_types: f.params.iter().map(|(t, _)| env.resolve_type(t)).collect(),
                    is_variadic: f.is_variadic,
                },
            );
            env.globals.insert(
                f.name.clone(),
                Type::FunctionPointer {
                    return_type: Box::new(env.resolve_type(&f.return_type)),
                    param_types: f
                        .params
                        .iter()
                        .map(|(t, _)| env.resolve_type(t))
                        .collect(),
                },
            );
        }

        env
    }

    fn register_function(&mut self, proto: &FunctionPrototype) {
        let sig = FunctionSig {
            return_type: self.resolve_type(&proto.return_type),
            param_types: proto
                .params
                .iter()
                .map(|(t, _)| self.resolve_type(t))
                .collect(),
            is_variadic: proto.is_variadic,
        };
        if let Some(existing) = self.functions.get(&proto.name) {
            if existing.param_types.len() != sig.param_types.len() && !existing.is_variadic {
                // Allow prototype refinement when a definition follows.
            }
        }
        self.functions.insert(proto.name.clone(), sig.clone());
        self.globals.insert(
            proto.name.clone(),
            Type::FunctionPointer {
                return_type: Box::new(sig.return_type.clone()),
                param_types: sig.param_types.clone(),
            },
        );
    }

    /// Resolve typedef and typeof wrappers to a concrete type.
    pub fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Typedef(name) => {
                if let Some(inner) = self.typedefs.get(name) {
                    self.resolve_type(inner)
                } else {
                    ty.clone()
                }
            }
            Type::Pointer(inner, q) => Type::qualified_ptr(self.resolve_type(inner), q.clone()),
            Type::Array(inner, n) => Type::Array(Box::new(self.resolve_type(inner)), *n),
            Type::TypeofExpr(_) => ty.clone(), // resolved at use site with expression context
            other => other.clone(),
        }
    }

    pub fn resolve_type_in_context(&self, ty: &Type, locals: &HashMap<String, Type>) -> Type {
        match ty {
            Type::TypeofExpr(expr) => self.expr_type(expr, locals),
            other => self.resolve_type(other),
        }
    }

    pub fn is_complete_type(&self, ty: &Type) -> bool {
        let ty = self.resolve_type(ty);
        match ty {
            Type::Struct(name) => self.structs.contains_key(&name),
            Type::Union(name) => self.unions.contains_key(&name),
            Type::Array(inner, 0) => self.is_complete_type(&inner),
            Type::Void => false,
            Type::TypeofExpr(_) => false,
            _ => true,
        }
    }

    pub fn is_integer_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Bool
                | Type::Char
                | Type::UnsignedChar
                | Type::Short
                | Type::UnsignedShort
                | Type::Int
                | Type::UnsignedInt
                | Type::Long
                | Type::UnsignedLong
                | Type::LongLong
                | Type::UnsignedLongLong
                | Type::Enum(_)
        )
    }

    pub fn is_floating_type(ty: &Type) -> bool {
        matches!(ty, Type::Float | Type::Double)
    }

    pub fn is_scalar_type(ty: &Type) -> bool {
        Self::is_integer_type(ty) || Self::is_floating_type(ty) || matches!(ty, Type::Pointer(_, ..))
    }

    /// C11 §6.3.1.1 integer promotions.
    pub fn integer_promotion(ty: &Type) -> Type {
        match ty {
            Type::Bool | Type::Char | Type::UnsignedChar | Type::Short | Type::UnsignedShort
            | Type::Enum(_) => Type::Int,
            other => other.clone(),
        }
    }

    /// Integer conversion rank (higher = wider).
    pub fn integer_rank(ty: &Type) -> u8 {
        match ty {
            Type::Bool => 1,
            Type::Char | Type::UnsignedChar => 2,
            Type::Short | Type::UnsignedShort => 3,
            Type::Int | Type::UnsignedInt | Type::Enum(_) => 4,
            Type::Long | Type::UnsignedLong => 5,
            Type::LongLong | Type::UnsignedLongLong => 6,
            _ => 0,
        }
    }

    pub fn is_unsigned_integer(ty: &Type) -> bool {
        matches!(
            ty,
            Type::UnsignedChar
                | Type::UnsignedShort
                | Type::UnsignedInt
                | Type::UnsignedLong
                | Type::UnsignedLongLong
                | Type::Bool
        )
    }

    /// C11 §6.3.1.8 usual arithmetic conversions (binary ops).
    pub fn usual_arithmetic_conversions(a: &Type, b: &Type) -> Type {
        if Self::is_floating_type(a) || Self::is_floating_type(b) {
            if matches!(a, Type::Double) || matches!(b, Type::Double) {
                return Type::Double;
            }
            return Type::Float;
        }
        let mut ta = Self::integer_promotion(a);
        let mut tb = Self::integer_promotion(b);
        let ra = Self::integer_rank(&ta);
        let rb = Self::integer_rank(&tb);
        if ra != rb {
            if ra < rb {
                ta = tb.clone();
            } else {
                tb = ta.clone();
            }
        } else if Self::is_unsigned_integer(&ta) != Self::is_unsigned_integer(&tb) {
            // If same rank, mixed signed/unsigned → unsigned
            if Self::is_unsigned_integer(&ta) {
                tb = ta.clone();
            } else {
                ta = tb.clone();
            }
        }
        ta
    }

    pub fn decay_array(ty: &Type) -> Type {
        match ty {
            Type::Array(inner, _) => Type::ptr((**inner).clone()),
            other => other.clone(),
        }
    }

    pub fn pointee_is_const(ty: &Type) -> bool {
        match ty {
            Type::Pointer(inner, q) => q.is_const || Self::pointee_is_const(inner),
            _ => false,
        }
    }

    pub fn types_compatible(&self, a: &Type, b: &Type) -> bool {
        let a = self.resolve_type(a);
        let b = self.resolve_type(b);
        match (&a, &b) {
            (Type::Void, Type::Void) => true,
            (Type::Int, Type::Int) | (Type::UnsignedInt, Type::UnsignedInt) => true,
            (Type::Enum(_), Type::Int) | (Type::Int, Type::Enum(_)) => true,
            (Type::Enum(a), Type::Enum(b)) if a == b => true,
            (Type::Char, Type::Char) | (Type::UnsignedChar, Type::UnsignedChar) => true,
            (Type::Short, Type::Short) | (Type::UnsignedShort, Type::UnsignedShort) => true,
            (Type::Long, Type::Long) | (Type::UnsignedLong, Type::UnsignedLong) => true,
            (Type::LongLong, Type::LongLong) | (Type::UnsignedLongLong, Type::UnsignedLongLong) => true,
            (Type::Float, Type::Float) | (Type::Double, Type::Double) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Pointer(a_i, _), Type::Pointer(b_i, _)) => self.types_compatible(a_i, b_i),
            (Type::Array(a_i, _), Type::Array(b_i, _)) => self.types_compatible(a_i, b_i),
            (Type::Struct(a), Type::Struct(b)) => a == b,
            (Type::Union(a), Type::Union(b)) => a == b,
            (Type::FunctionPointer { return_type: ar, param_types: ap }, Type::FunctionPointer { return_type: br, param_types: bp }) => {
                ap.len() == bp.len()
                    && self.types_compatible(ar, br)
                    && ap.iter().zip(bp.iter()).all(|(x, y)| self.types_compatible(x, y))
            }
            _ if Self::is_integer_type(&a) && Self::is_integer_type(&b) => {
                // Allow implicit integer conversions of same signedness family
                Self::integer_rank(&a) == Self::integer_rank(&b)
                    || (Self::integer_rank(&a) < Self::integer_rank(&b)
                        && !Self::is_unsigned_integer(&b))
                    || (Self::integer_rank(&b) < Self::integer_rank(&a)
                        && !Self::is_unsigned_integer(&a))
            }
            _ => false,
        }
    }

    pub fn is_assign_compatible(&self, lhs: &Type, rhs: &Type) -> bool {
        let lhs = self.resolve_type(lhs);
        let mut rhs = self.resolve_type(rhs);
        rhs = Self::decay_array(&rhs);
        if self.types_compatible(&lhs, &rhs) {
            return true;
        }
        // Pointer assignment: null (0), void*, compatible pointees
        if let Type::Pointer(l_inner, _) = &lhs {
            if let Type::Pointer(r_inner, _) = &rhs {
                return self.types_compatible(l_inner, r_inner)
                    || matches!(l_inner.as_ref(), Type::Void)
                    || matches!(r_inner.as_ref(), Type::Void);
            }
            if Self::is_integer_type(&rhs) {
                return true; // null pointer constant
            }
        }
        // Array decay already handled; integer to pointer not allowed except null
        if let Type::Array(inner, _) = &lhs {
            if matches!(inner.as_ref(), Type::Char)
                && matches!(&rhs, Type::Pointer(p, _) if matches!(p.as_ref(), Type::Char))
            {
                return true;
            }
        }
        if Self::is_floating_type(&lhs) && Self::is_integer_type(&rhs) {
            return true;
        }
        if Self::is_integer_type(&lhs) && Self::is_integer_type(&rhs) {
            return true;
        }
        if Self::is_floating_type(&lhs) && Self::is_floating_type(&rhs) {
            return true;
        }
        false
    }

    pub fn expr_type(&self, expr: &Expr, locals: &HashMap<String, Type>) -> Type {
        match expr {
            Expr::Constant(_) => Type::Int,
            Expr::FloatConstant(_) => Type::Double,
            Expr::StringLiteral(_) => Type::ptr(Type::Char),
            Expr::Variable(name) => {
                if let Some(t) = locals.get(name) {
                    return t.clone();
                }
                if let Some(t) = self.globals.get(name) {
                    return t.clone();
                }
                if self.enum_constants.contains(name) {
                    return Type::Int;
                }
                Type::Int
            }
            Expr::Binary { left, op, right } => self.binary_type(left, op, right, locals),
            Expr::Unary { op, expr } => self.unary_type(op, expr, locals),
            Expr::PostfixIncrement(expr) | Expr::PostfixDecrement(expr)
            | Expr::PrefixIncrement(expr) | Expr::PrefixDecrement(expr) => {
                self.expr_type(expr, locals)
            }
            Expr::Cast(ty, _) => self.resolve_type(ty),
            Expr::Member { expr, member } => self.member_type(expr, member, locals, false),
            Expr::PtrMember { expr, member } => self.member_type(expr, member, locals, true),
            Expr::Index { array, .. } => {
                let ty = self.expr_type(array, locals);
                match ty {
                    Type::Array(inner, _) => *inner,
                    Type::Pointer(inner, ..) => *inner,
                    _ => Type::Int,
                }
            }
            Expr::Call { func, .. } => self.call_return_type(func, locals),
            Expr::SizeOf(_) | Expr::SizeOfExpr(_) | Expr::AlignOf(_) => Type::Long,
            Expr::Conditional { then_expr, else_expr, .. } => {
                let t = self.expr_type(then_expr, locals);
                let e = self.expr_type(else_expr, locals);
                if Self::is_arithmetic(&t) && Self::is_arithmetic(&e) {
                    Self::usual_arithmetic_conversions(&t, &e)
                } else if self.types_compatible(&t, &e) {
                    t
                } else {
                    t
                }
            }
            Expr::CompoundLiteral { r#type, .. } => self.resolve_type(r#type),
            Expr::Comma(exprs) => exprs
                .last()
                .map(|e| self.expr_type(e, locals))
                .unwrap_or(Type::Int),
            Expr::InitList(_) => Type::Int,
            Expr::BuiltinOffsetof { .. } => Type::Long,
            Expr::VaArg { r#type, .. } => self.resolve_type(r#type),
            Expr::Expect { expr, .. } => self.expr_type(expr, locals),
            Expr::Generic { controlling, associations } => {
                let ctrl = self.expr_type(controlling, locals);
                for (ty, e) in associations {
                    if let Some(t) = ty {
                        if self.types_compatible(&ctrl, t) {
                            return self.expr_type(e, locals);
                        }
                    }
                }
                for (ty, e) in associations {
                    if ty.is_none() {
                        return self.expr_type(e, locals);
                    }
                }
                Type::Int
            }
            Expr::StmtExpr(stmts) => {
                use crate::Stmt;
                if let Some(Stmt::Expr(e)) = stmts.last() {
                    self.expr_type(e, locals)
                } else {
                    Type::Int
                }
            }
            Expr::LabelAddr(_) => Type::Pointer(Box::new(Type::Void), TypeQualifiers::default()),
        }
    }

    fn is_arithmetic(ty: &Type) -> bool {
        Self::is_integer_type(ty) || Self::is_floating_type(ty)
    }

    fn binary_type(
        &self,
        left: &Expr,
        op: &BinaryOp,
        right: &Expr,
        locals: &HashMap<String, Type>,
    ) -> Type {
        if matches!(
            op,
            BinaryOp::Assign
                | BinaryOp::AddAssign
                | BinaryOp::SubAssign
                | BinaryOp::MulAssign
                | BinaryOp::DivAssign
                | BinaryOp::ModAssign
                | BinaryOp::BitwiseAndAssign
                | BinaryOp::BitwiseOrAssign
                | BinaryOp::BitwiseXorAssign
                | BinaryOp::ShiftLeftAssign
                | BinaryOp::ShiftRightAssign
        ) {
            return self.expr_type(left, locals);
        }
        if matches!(
            op,
            BinaryOp::EqualEqual
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
                | BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr
        ) {
            return Type::Int;
        }
        let lt = self.expr_type(left, locals);
        let rt = self.expr_type(right, locals);
        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            if let Type::Pointer(inner, ..) = &lt {
                if Self::is_integer_type(&rt) {
                    return Type::Pointer(inner.clone(), TypeQualifiers::default());
                }
            }
            if let Type::Pointer(inner, ..) = &rt {
                if Self::is_integer_type(&lt) && matches!(op, BinaryOp::Add) {
                    return Type::Pointer(inner.clone(), TypeQualifiers::default());
                }
            }
            if matches!(op, BinaryOp::Sub) {
                if let (Type::Pointer(a, ..), Type::Pointer(b, ..)) = (&lt, &rt) {
                    if self.types_compatible(a, b) {
                        return Type::Long;
                    }
                }
            }
        }
        if Self::is_floating_type(&lt) || Self::is_floating_type(&rt) {
            return Self::usual_arithmetic_conversions(&lt, &rt);
        }
        if Self::is_integer_type(&lt) && Self::is_integer_type(&rt) {
            return Self::usual_arithmetic_conversions(&lt, &rt);
        }
        lt
    }

    fn unary_type(&self, op: &UnaryOp, expr: &Expr, locals: &HashMap<String, Type>) -> Type {
        let ty = self.expr_type(expr, locals);
        match op {
            UnaryOp::AddrOf => Type::ptr(ty),
            UnaryOp::Deref => match ty {
                Type::Pointer(inner, ..) => *inner,
                Type::Array(inner, _) => *inner,
                _ => Type::Int,
            },
            UnaryOp::LogicalNot => Type::Int,
            _ if Self::is_integer_type(&ty) || Self::is_floating_type(&ty) => {
                Self::integer_promotion(&ty)
            }
            _ => ty,
        }
    }

    fn member_type(
        &self,
        expr: &Expr,
        member: &str,
        locals: &HashMap<String, Type>,
        through_ptr: bool,
    ) -> Type {
        let mut ty = self.expr_type(expr, locals);
        if through_ptr {
            if let Type::Pointer(inner, ..) = ty {
                ty = *inner;
            }
        }
        match ty {
            Type::Struct(name) => self
                .structs
                .get(&name)
                .and_then(|s| s.fields.iter().find(|f| f.name == member))
                .map(|f| f.field_type.clone())
                .unwrap_or(Type::Int),
            Type::Union(name) => self
                .unions
                .get(&name)
                .and_then(|u| u.fields.iter().find(|f| f.name == member))
                .map(|f| f.field_type.clone())
                .unwrap_or(Type::Int),
            _ => Type::Int,
        }
    }

    fn call_return_type(&self, func: &Expr, locals: &HashMap<String, Type>) -> Type {
        match func {
            Expr::Variable(name) => self
                .functions
                .get(name)
                .map(|s| s.return_type.clone())
                .unwrap_or(Type::Int),
            _ => {
                let ft = self.expr_type(func, locals);
                if let Type::FunctionPointer { return_type, .. } = ft {
                    *return_type
                } else {
                    Type::Int
                }
            }
        }
    }

    pub fn check_call(
        &self,
        func: &Expr,
        args: &[Expr],
        locals: &HashMap<String, Type>,
    ) -> Result<(), String> {
        let (sig, name) = match func {
            Expr::Variable(name) => (
                self.functions.get(name).cloned(),
                Some(name.clone()),
            ),
            _ => (None, None),
        };
        let Some(sig) = sig else {
            return Ok(()); // indirect call — limited checking
        };
        let required = sig.param_types.len();
        if args.len() < required || (!sig.is_variadic && args.len() > required) {
            return Err(format!(
                "Call to '{}' expects {} argument(s){}, got {}",
                name.unwrap_or_default(),
                required,
                if sig.is_variadic { " (variadic)" } else { "" },
                args.len()
            ));
        }
        for (i, arg) in args.iter().enumerate().take(required) {
            let expected = &sig.param_types[i];
            let got = self.expr_type(arg, locals);
            if !self.is_assign_compatible(expected, &got) {
                return Err(format!(
                    "Argument {} to '{}': expected {:?}, got {:?}",
                    i + 1,
                    name.unwrap_or_default(),
                    expected,
                    got
                ));
            }
        }
        Ok(())
    }

    pub fn is_lvalue(expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Variable(_)
                | Expr::Index { .. }
                | Expr::Member { .. }
                | Expr::PtrMember { .. }
                | Expr::Unary { op: UnaryOp::Deref, .. }
        )
    }

    pub fn max_bitfield_width(field_type: &Type) -> usize {
        match field_type {
            Type::Char | Type::UnsignedChar | Type::Bool => 8,
            Type::Short | Type::UnsignedShort => 16,
            Type::Int | Type::UnsignedInt | Type::Enum(_) => 32,
            Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong => 64,
            _ => 0,
        }
    }

    pub fn validate_bitfield(field: &StructField) -> Result<(), String> {
        if let Some(w) = field.bit_width {
            let max = Self::max_bitfield_width(&field.field_type);
            if max == 0 {
                return Err(format!(
                    "Bit-field '{}' has non-integral type",
                    field.name
                ));
            }
            if w == 0 || w > max {
                return Err(format!(
                    "Bit-field '{}' width {} exceeds type width {}",
                    field.name, w, max
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_promotion_char_to_int() {
        assert_eq!(TypeEnv::integer_promotion(&Type::Char), Type::Int);
    }

    #[test]
    fn usual_arithmetic_unsigned_wins() {
        let t = TypeEnv::usual_arithmetic_conversions(&Type::Int, &Type::UnsignedInt);
        assert_eq!(t, Type::UnsignedInt);
    }

    #[test]
    fn array_decays_to_pointer() {
        let t = TypeEnv::decay_array(&Type::Array(Box::new(Type::Int), 10));
        assert!(matches!(t, Type::Pointer(_, ..)));
    }
}
