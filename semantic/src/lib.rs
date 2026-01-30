use model::{Program, Function, Stmt, Expr, Type};
use std::collections::HashMap;

pub struct SemanticAnalyzer {
    symbols: HashMap<String, Type>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<(), String> {
        for function in &program.functions {
            self.analyze_function(function)?;
        }
        Ok(())
    }

    fn analyze_function(&mut self, function: &Function) -> Result<(), String> {
        self.symbols.clear();
        for (t, name) in &function.params {
            self.symbols.insert(name.clone(), t.clone());
        }
        self.analyze_stmt(&Stmt::Block(function.body.clone()))?;
        Ok(())
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Declaration { r#type, name, init } => {
                if self.symbols.contains_key(name) {
                    return Err(format!("Redeclaration of variable {}", name));
                }
                self.symbols.insert(name.clone(), r#type.clone());
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
                // Nested scopes could be handled here by saving/restoring symbols
                // For now, keeping it simple as the original IR lowerer did
                for s in &block.statements {
                    self.analyze_stmt(s)?;
                }
            }
            Stmt::If { cond, then_branch, else_branch } => {
                self.analyze_expr(cond)?;
                self.analyze_stmt(then_branch)?;
                if let Some(else_stmt) = else_branch {
                    self.analyze_stmt(else_stmt)?;
                }
            }
            Stmt::While { cond, body } => {
                self.analyze_expr(cond)?;
                self.analyze_stmt(body)?;
            }
            Stmt::DoWhile { body, cond } => {
                self.analyze_stmt(body)?;
                self.analyze_expr(cond)?;
            }
            Stmt::For { init, cond, post, body } => {
                if let Some(e) = init {
                    self.analyze_expr(e)?;
                }
                if let Some(e) = cond {
                    self.analyze_expr(e)?;
                }
                if let Some(e) = post {
                    self.analyze_expr(e)?;
                }
                self.analyze_stmt(body)?;
            }
        }
        Ok(())
    }

    fn analyze_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Variable(name) => {
                if !self.symbols.contains_key(name) {
                    return Err(format!("Undeclared variable {}", name));
                }
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left)?;
                self.analyze_expr(right)?;
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr)?;
            }
            Expr::Constant(_) => {}
            Expr::StringLiteral(_) => {}
            Expr::Index { array, index } => {
                self.analyze_expr(array)?;
                self.analyze_expr(index)?;
                // Future: check if 'array' is actually an array type
            }
            Expr::Call { name: _, args } => {
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
        }
        Ok(())
    }
}
