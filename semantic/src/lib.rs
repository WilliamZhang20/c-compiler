use model::{Program, Function, Stmt, Expr, Type, BinaryOp};
use std::collections::HashMap;

pub struct SemanticAnalyzer {
    global_scope: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
    const_vars: HashMap<String, bool>, // Track const-qualified variables
    volatile_vars: HashMap<String, bool>, // Track volatile-qualified variables  
    structs: HashMap<String, model::StructDef>,
    unions: HashMap<String, model::UnionDef>,
    enum_constants: HashMap<String, i64>, // enum constant name => value
    loop_depth: usize,
    in_switch: bool,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            global_scope: HashMap::new(),
            scopes: Vec::new(),
            const_vars: HashMap::new(),
            volatile_vars: HashMap::new(),
            structs: HashMap::new(),
            unions: HashMap::new(),
            enum_constants: HashMap::new(),
            loop_depth: 0,
            in_switch: false,
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<(), String> {
        self.global_scope.clear();
        self.const_vars.clear();
        self.volatile_vars.clear();
        self.structs.clear();
        self.unions.clear();
        self.enum_constants.clear();
        
        for s_def in &program.structs {
            self.structs.insert(s_def.name.clone(), s_def.clone());
        }
        
        for u_def in &program.unions {
            self.unions.insert(u_def.name.clone(), u_def.clone());
        }
        
        // Register all enum constants
        for enum_def in &program.enums {
            for (const_name, const_value) in &enum_def.constants {
                if self.enum_constants.contains_key(const_name) {
                    return Err(format!("Redeclaration of enum constant {}", const_name));
                }
                self.enum_constants.insert(const_name.clone(), *const_value);
            }
        }
        
        for global in &program.globals {
            if self.global_scope.contains_key(&global.name) {
                // Allow redeclarations - just skip this one (common with extern vs definition)
                continue;
            }
            
            // Validate restrict on pointers only
            if global.qualifiers.is_restrict && !matches!(global.r#type, Type::Pointer(_)) {
                return Err(format!("'restrict' can only be applied to pointer types"));
            }
            
            self.global_scope.insert(global.name.clone(), global.r#type.clone());
            if global.qualifiers.is_const {
                self.const_vars.insert(global.name.clone(), true);
            }
            if global.qualifiers.is_volatile {
                self.volatile_vars.insert(global.name.clone(), true);
            }
        }
        
        // Add function names as function pointers to global scope
        for function in &program.functions {
            let func_type = Type::FunctionPointer {
                return_type: Box::new(function.return_type.clone()),
                param_types: function.params.iter().map(|(t, _)| t.clone()).collect(),
            };
            if self.global_scope.contains_key(&function.name) {
                return Err(format!("Redeclaration of function {}", function.name));
            }
            self.global_scope.insert(function.name.clone(), func_type);
        }

        for function in &program.functions {
            self.analyze_function(function)?;
        }
        Ok(())
    }

    fn analyze_function(&mut self, function: &Function) -> Result<(), String> {
        self.scopes = vec![self.global_scope.clone()];
        self.loop_depth = 0;
        self.in_switch = false;
        
        self.enter_scope();
        for (t, name) in &function.params {
            self.add_symbol(name.clone(), t.clone());
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

    fn add_symbol(&mut self, name: String, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    fn lookup_symbol(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Declaration { r#type, qualifiers, name, init } => {
                // Validate restrict on pointers only
                if qualifiers.is_restrict && !matches!(r#type, Type::Pointer(_)) {
                    return Err(format!("'restrict' can only be applied to pointer types"));
                }
                
                self.add_symbol(name.clone(), r#type.clone());
                if qualifiers.is_const {
                    self.const_vars.insert(name.clone(), true);
                }
                if qualifiers.is_volatile {
                    self.volatile_vars.insert(name.clone(), true);
                }
                
                if let Some(expr) = init {
                    self.analyze_expr(expr)?;
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e)?;
                }
            }
            Stmt::Expr(expr) => {
                self.analyze_expr(expr)?;
            }
            Stmt::Block(block) => {
                self.enter_scope();
                for s in &block.statements {
                    self.analyze_stmt(s)?;
                }
                self.exit_scope();
            }            Stmt::MultiDecl(stmts) => {
                // Flat multi-variable declaration — no new scope; all declared names
                // are visible in the surrounding block.
                for s in stmts {
                    self.analyze_stmt(s)?
                }
            }            Stmt::If { cond, then_branch, else_branch } => {
                self.analyze_expr(cond)?;
                self.analyze_stmt(then_branch)?;
                if let Some(else_stmt) = else_branch {
                    self.analyze_stmt(else_stmt)?;
                }
            }
            Stmt::While { cond, body } => {
                self.analyze_expr(cond)?;
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
            }
            Stmt::DoWhile { body, cond } => {
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
                self.analyze_expr(cond)?;
            }
            Stmt::For { init, cond, post, body } => {
                if let Some(stmt) = init {
                    self.analyze_stmt(stmt)?;
                }
                if let Some(e) = cond {
                    self.analyze_expr(e)?;
                }
                if let Some(e) = post {
                    self.analyze_expr(e)?;
                }
                self.loop_depth += 1;
                self.analyze_stmt(body)?;
                self.loop_depth -= 1;
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
                self.analyze_expr(cond)?;
                let old_switch = self.in_switch;
                self.in_switch = true;
                self.analyze_stmt(body)?;
                self.in_switch = old_switch;
            }
            Stmt::Case(expr) => {
                if !self.in_switch {
                    return Err("'case' label not within a switch statement".to_string());
                }
                self.analyze_expr(expr)?;
            }
            Stmt::Default => {
                if !self.in_switch {
                    return Err("'default' label not within a switch statement".to_string());
                }
            }
            Stmt::Goto(_label) => {
                // Label references will be validated by IR lowerer
                // We just need to accept goto statements here
            }
            Stmt::Label(_name) => {
                // Labels are always valid, IR lowerer will track them
            }
            Stmt::InlineAsm { outputs, inputs, .. } => {
                // Validate that output and input expressions are valid
                for operand in outputs {
                    self.analyze_expr(&operand.expr)?;
                }
                for operand in inputs {
                    self.analyze_expr(&operand.expr)?;
                }
            }
        }
        Ok(())
    }

    fn analyze_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Variable(name) => {
                // Check if it's a variable or an enum constant
                if self.lookup_symbol(name).is_none() && !self.enum_constants.contains_key(name) {
                    return Err(format!("Undeclared variable {}", name));
                }
            }
            Expr::Binary { left, op, right } => {
                self.analyze_expr(left)?;
                self.analyze_expr(right)?;
                
                // Check for assignment to const variable
                if matches!(op, BinaryOp::Assign | BinaryOp::AddAssign | BinaryOp::SubAssign 
                    | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign 
                    | BinaryOp::BitwiseAndAssign | BinaryOp::BitwiseOrAssign 
                    | BinaryOp::BitwiseXorAssign | BinaryOp::ShiftLeftAssign 
                    | BinaryOp::ShiftRightAssign) 
                {
                    if let Expr::Variable(name) = left.as_ref() {
                        if self.const_vars.get(name) == Some(&true) {
                            return Err(format!("Cannot assign to const variable '{}'", name));
                        }
                    }
                }
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr)?;
            }
            Expr::PostfixIncrement(expr) | Expr::PostfixDecrement(expr) 
            | Expr::PrefixIncrement(expr) | Expr::PrefixDecrement(expr) => {
                self.analyze_expr(expr)?;
                // Check for increment/decrement of const variable
                if let Expr::Variable(name) = expr.as_ref() {
                    if self.const_vars.get(name) == Some(&true) {
                        return Err(format!("Cannot modify const variable '{}'", name));
                    }
                }
            }
            Expr::Constant(_) => {}
            Expr::FloatConstant(_) => {}
            Expr::StringLiteral(_) => {}
            Expr::Index { array, index } => {
                self.analyze_expr(array)?;
                self.analyze_expr(index)?;
            }
            Expr::Call { func, args } => {
                // For direct function calls (Variable), allow undeclared functions
                // to support separate compilation
                if !matches!(**func, Expr::Variable(_)) {
                    self.analyze_expr(func)?;
                }
                for arg in args {
                    self.analyze_expr(arg)?;
                }
            }
            Expr::SizeOf(_) => {}
            Expr::SizeOfExpr(expr) => {
                self.analyze_expr(expr)?;
            }
            Expr::Cast(_, expr) => {
                self.analyze_expr(expr)?;
            }
            Expr::Member { expr, member: _ } => {
                self.analyze_expr(expr)?;
            }
            Expr::PtrMember { expr, member: _ } => {
                self.analyze_expr(expr)?;
            }
            Expr::Conditional { condition, then_expr, else_expr } => {
                self.analyze_expr(condition)?;
                self.analyze_expr(then_expr)?;
                self.analyze_expr(else_expr)?;
            }
            Expr::InitList(items) => {
                for item in items {
                    self.analyze_expr(&item.value)?;
                }
            }
        }
        Ok(())
    }
}
