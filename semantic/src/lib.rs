use model::{Program, Function, Stmt, Expr, Type, BinaryOp, TypeEnv, TypeQualifiers};
use std::collections::{HashMap, HashSet};

pub struct SemanticAnalyzer {
    type_env: TypeEnv,
    scopes: Vec<HashMap<String, Type>>,
    const_vars: HashSet<String>,
    volatile_vars: HashSet<String>,
    loop_depth: usize,
    in_switch: bool,
    current_return_type: Option<Type>,
    case_values: HashSet<i64>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            type_env: TypeEnv::from_program(&Program {
                functions: vec![],
                globals: vec![],
                structs: vec![],
                unions: vec![],
                enums: vec![],
                prototypes: vec![],
                forward_structs: vec![],
                typedefs: HashMap::new(),
            }),
            scopes: Vec::new(),
            const_vars: HashSet::new(),
            volatile_vars: HashSet::new(),
            loop_depth: 0,
            in_switch: false,
            current_return_type: None,
            case_values: HashSet::new(),
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<(), String> {
        self.type_env = TypeEnv::from_program(program);
        self.const_vars.clear();
        self.volatile_vars.clear();
        self.scopes.clear();

        for s_def in &program.structs {
            for field in &s_def.fields {
                TypeEnv::validate_bitfield(field)?;
            }
        }

        for enum_def in &program.enums {
            let mut seen = HashSet::new();
            for (const_name, _) in &enum_def.constants {
                if !seen.insert(const_name.clone()) {
                    return Err(format!("Redeclaration of enum constant {}", const_name));
                }
            }
        }

        for global in &program.globals {
            if global.qualifiers.is_restrict && !matches!(global.r#type, Type::Pointer(_, ..)) {
                return Err(format!(
                    "'restrict' can only be applied to pointer types on '{}'",
                    global.name
                ));
            }
            if global.qualifiers.is_const {
                self.const_vars.insert(global.name.clone());
            }
            if global.qualifiers.is_volatile {
                self.volatile_vars.insert(global.name.clone());
            }
            if let Some(init) = &global.init {
                let ty = self.type_env.resolve_type(&global.r#type);
                self.check_init_compatible(&ty, init)?;
            }
        }

        for function in &program.functions {
            if self.type_env.functions.contains_key(&function.name) {
                // Definition may follow prototype — validated at registration time.
            }
            self.analyze_function(function)?;
        }
        Ok(())
    }

    fn locals(&self) -> HashMap<String, Type> {
        let mut map = HashMap::new();
        for scope in &self.scopes {
            for (k, v) in scope {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }

    fn analyze_function(&mut self, function: &Function) -> Result<(), String> {
        self.scopes.clear();
        self.loop_depth = 0;
        self.in_switch = false;
        self.current_return_type = Some(self.type_env.resolve_type(&function.return_type));

        self.enter_scope();
        for (t, name) in &function.params {
            let resolved = self.type_env.resolve_type(t);
            if !self.type_env.is_complete_type(&resolved) {
                return Err(format!(
                    "Parameter '{}' has incomplete type in function '{}'",
                    name, function.name
                ));
            }
            self.declare_local(name, resolved, TypeQualifiers::default(), false)?;
        }
        self.analyze_stmt(&Stmt::Block(function.body.clone()))?;
        self.exit_scope();
        Ok(())
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_local(
        &mut self,
        name: &str,
        ty: Type,
        qualifiers: model::TypeQualifiers,
        allow_shadow: bool,
    ) -> Result<(), String> {
        if qualifiers.is_restrict && !matches!(ty, Type::Pointer(_, ..)) {
            return Err(format!("'restrict' can only be applied to pointer types"));
        }
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) && !allow_shadow {
                return Err(format!("Redeclaration of '{}'", name));
            }
            scope.insert(name.to_string(), ty);
        }
        if qualifiers.is_const {
            self.const_vars.insert(name.to_string());
        }
        if qualifiers.is_volatile {
            self.volatile_vars.insert(name.to_string());
        }
        Ok(())
    }

    fn lookup_symbol(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        self.type_env.globals.get(name).cloned()
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Declaration { r#type, qualifiers, name, init } => {
                let locals = self.locals();
                let resolved = self.type_env.resolve_type_in_context(r#type, &locals);
                if !self.type_env.is_complete_type(&resolved) {
                    return Err(format!("Variable '{}' has incomplete type", name));
                }
                self.declare_local(name, resolved.clone(), qualifiers.clone(), true)?;
                if let Some(expr) = init {
                    self.check_init_compatible(&resolved, expr)?;
                }
            }
            Stmt::Return(expr) => {
                let ret_ty = self.current_return_type.clone();
                if let Some(ret_ty) = ret_ty {
                    if ret_ty == Type::Void {
                        if expr.is_some() {
                            return Err("Return with value in void function".to_string());
                        }
                    } else if let Some(e) = expr {
                        let got = self.check_expr(e)?;
                        if !self.type_env.is_assign_compatible(&ret_ty, &got) {
                            return Err(format!(
                                "Return type mismatch: expected {:?}, got {:?}",
                                ret_ty, got
                            ));
                        }
                    }
                } else if let Some(e) = expr {
                    self.check_expr(e)?;
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr)?;
            }
            Stmt::Block(block) => {
                self.enter_scope();
                for s in &block.statements {
                    self.analyze_stmt(s)?;
                }
                self.exit_scope();
            }
            Stmt::MultiDecl(stmts) => {
                for s in stmts {
                    self.analyze_stmt(s)?;
                }
            }
            Stmt::If { cond, then_branch, else_branch } => {
                self.check_expr(cond)?;
                self.analyze_stmt(then_branch)?;
                if let Some(else_stmt) = else_branch {
                    self.analyze_stmt(else_stmt)?;
                }
            }
            Stmt::While { cond, body } => {
                self.check_expr(cond)?;
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
            }
            Stmt::DoWhile { body, cond } => {
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
                self.check_expr(cond)?;
            }
            Stmt::For { init, cond, post, body } => {
                self.enter_scope();
                if let Some(stmt) = init {
                    self.analyze_stmt(stmt)?;
                }
                if let Some(e) = cond {
                    self.check_expr(e)?;
                }
                if let Some(e) = post {
                    self.check_expr(e)?;
                }
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
                self.exit_scope();
            }
            Stmt::Break => {
                if self.loop_depth == 0 && !self.in_switch {
                    return Err("'break' statement not within a loop or switch".to_string());
                }
            }
            Stmt::Continue => {
                if self.loop_depth == 0 {
                    return Err("'continue' statement not within a loop".to_string());
                }
            }
            Stmt::Switch { cond, body } => {
                self.check_expr(cond)?;
                let old_switch = self.in_switch;
                let old_cases = std::mem::take(&mut self.case_values);
                self.in_switch = true;
                self.analyze_stmt(body)?;
                self.in_switch = old_switch;
                self.case_values = old_cases;
            }
            Stmt::Case(expr) => {
                if !self.in_switch {
                    return Err("'case' label not within a switch statement".to_string());
                }
                if let Expr::Constant(v) = expr {
                    if !self.case_values.insert(*v) {
                        return Err(format!("Duplicate case value {}", v));
                    }
                }
                self.check_expr(expr)?;
            }
            Stmt::Default => {
                if !self.in_switch {
                    return Err("'default' label not within a switch statement".to_string());
                }
            }
            Stmt::Goto(_label) => {}
            Stmt::ComputedGoto(expr) => {
                let ty = self.check_expr(expr)?;
                match ty {
                    Type::Pointer(_, ..) => {}
                    _ => {
                        return Err(format!(
                            "Computed goto requires pointer type, got {:?}",
                            ty
                        ));
                    }
                }
            }
            Stmt::Label(_name) => {}
            Stmt::InlineAsm { outputs, inputs, .. } => {
                for operand in outputs {
                    self.check_expr(&operand.expr)?;
                }
                for operand in inputs {
                    self.check_expr(&operand.expr)?;
                }
            }
        }
        Ok(())
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Type, String> {
        let locals = self.locals();
        let ty = self.type_env.expr_type(expr, &locals);

        match expr {
            Expr::Variable(name) => {
                if self.lookup_symbol(name).is_none()
                    && !self.type_env.enum_constants.contains(name)
                {
                    return Err(format!("Undeclared variable {}", name));
                }
            }
            Expr::Binary { left, op, right } => {
                self.check_expr(left)?;
                self.check_expr(right)?;
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
                    if !TypeEnv::is_lvalue(left) {
                        return Err("Assignment requires an lvalue".to_string());
                    }
                    self.check_const_assignment(left)?;
                    let lhs_ty = self.type_env.expr_type(left, &locals);
                    let rhs_ty = self.type_env.expr_type(right, &locals);
                    if !self.type_env.is_assign_compatible(&lhs_ty, &rhs_ty) {
                        return Err(format!(
                            "Incompatible assignment: {:?} = {:?}",
                            lhs_ty, rhs_ty
                        ));
                    }
                    if TypeEnv::pointee_is_const(&lhs_ty) {
                        return Err("Cannot assign through pointer to const".to_string());
                    }
                }
            }
            Expr::Unary { op: model::UnaryOp::Deref, expr: inner } => {
                self.check_expr(inner)?;
                if TypeEnv::pointee_is_const(&ty) {
                    // read-only deref is fine; assignment checked elsewhere
                    let _ = inner;
                }
            }
            Expr::Unary { expr, .. } => {
                self.check_expr(expr)?;
            }
            Expr::PostfixIncrement(expr)
            | Expr::PostfixDecrement(expr)
            | Expr::PrefixIncrement(expr)
            | Expr::PrefixDecrement(expr) => {
                if !TypeEnv::is_lvalue(expr) {
                    return Err("Increment/decrement requires an lvalue".to_string());
                }
                self.check_const_assignment(expr)?;
                self.check_expr(expr)?;
            }
            Expr::Call { func, args } => {
                self.type_env.check_call(func, args, &locals)?;
                if !matches!(**func, Expr::Variable(_)) {
                    self.check_expr(func)?;
                }
                for arg in args {
                    self.check_expr(arg)?;
                }
            }
            Expr::Cast(cast_ty, inner) => {
                self.check_expr(inner)?;
                let _ = self.type_env.resolve_type(cast_ty);
            }
            Expr::LabelAddr(label) => {
                // Label must exist in function — validated at IR lowering
                let _ = label;
            }
            _ => {
                self.check_expr_children(expr)?;
            }
        }
        Ok(ty)
    }

    fn check_expr_children(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Binary { left, right, .. } => {
                self.check_expr(left)?;
                self.check_expr(right)?;
            }
            Expr::Unary { expr, .. } => {
                self.check_expr(expr)?;
            }
            Expr::Index { array, index } => {
                self.check_expr(array)?;
                self.check_expr(index)?;
            }
            Expr::Member { expr, .. } | Expr::PtrMember { expr, .. } => {
                self.check_expr(expr)?;
            }
            Expr::Conditional { condition, then_expr, else_expr } => {
                self.check_expr(condition)?;
                self.check_expr(then_expr)?;
                self.check_expr(else_expr)?;
            }
            Expr::CompoundLiteral { init, .. } => {
                for item in init {
                    self.check_expr(&item.value)?;
                }
            }
            Expr::StmtExpr(stmts) => {
                for stmt in stmts {
                    self.analyze_stmt(stmt)?;
                }
            }
            Expr::Comma(exprs) => {
                for e in exprs {
                    self.check_expr(e)?;
                }
            }
            Expr::InitList(items) => {
                for item in items {
                    self.check_expr(&item.value)?;
                }
            }
            Expr::Expect { expr, expected } => {
                self.check_expr(expr)?;
                self.check_expr(expected)?;
            }
            Expr::Generic { controlling, associations } => {
                self.check_expr(controlling)?;
                for (_, e) in associations {
                    self.check_expr(e)?;
                }
            }
            Expr::VaArg { list, .. } => {
                self.check_expr(list)?;
            }
            Expr::SizeOfExpr(e) => {
                self.check_expr(e)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_const_assignment(&self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Variable(name) => {
                if self.const_vars.contains(name) {
                    return Err(format!("Cannot modify const variable '{}'", name));
                }
            }
            Expr::Unary { op: model::UnaryOp::Deref, expr: inner } => {
                let locals = self.locals();
                let ptr_ty = self.type_env.expr_type(inner, &locals);
                if TypeEnv::pointee_is_const(&ptr_ty) {
                    return Err("Cannot assign through pointer to const".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_init_compatible(&mut self, target: &Type, init: &Expr) -> Result<(), String> {
        match init {
            Expr::InitList(_) => Ok(()),
            _ => {
                let got = self.check_expr(init)?;
                if !self.type_env.is_assign_compatible(target, &got) {
                    return Err(format!(
                        "Initializer incompatible: expected {:?}, got {:?}",
                        target, got
                    ));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(src: &str) -> Result<(), String> {
        let tokens = lexer::lex(src).unwrap();
        let program = parser::parse_tokens(&tokens).unwrap();
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.analyze(&program)
    }

    #[test]
    fn valid_simple_program() {
        assert!(analyze("int main() { return 0; }").is_ok());
    }

    #[test]
    fn error_undeclared_variable() {
        assert!(analyze("int main() { return x; }").is_err());
    }

    #[test]
    fn error_const_assignment() {
        assert!(analyze("int main() { const int x = 5; x = 10; return x; }").is_err());
    }

    #[test]
    fn error_wrong_call_arity() {
        assert!(analyze("int foo(int a) { return a; } int main() { return foo(1, 2); }").is_err());
    }

    #[test]
    fn valid_prototype_then_definition() {
        assert!(analyze(
            "int add(int, int); int add(int a, int b) { return a + b; } int main() { return add(1, 2); }"
        )
        .is_ok());
    }

    #[test]
    fn error_return_type_mismatch() {
        assert!(analyze("int main() { return; }").is_ok());
        assert!(analyze("void main(void) { return 1; }").is_err());
    }

    #[test]
    fn error_duplicate_case() {
        assert!(analyze(
            "int main() { int x = 1; switch (x) { case 1: break; case 1: break; } return 0; }"
        )
        .is_err());
    }
}
