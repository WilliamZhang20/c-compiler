use model::{Type, Program as AstProgram, Function as AstFunction, Expr as AstExpr};
use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, BasicBlock, Function, IRProgram, Instruction, Terminator, Operand};

/// Main AST to IR lowering engine with SSA construction
pub struct Lowerer {
    pub(crate) next_var: usize,
    pub(crate) next_block: usize,
    pub(crate) current_def: HashMap<String, VarId>,
    pub(crate) symbol_table: HashMap<String, Type>,
    pub(crate) variable_defs: HashMap<String, HashMap<BlockId, VarId>>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) current_block: Option<BlockId>,
    pub(crate) incomplete_phis: HashMap<BlockId, HashMap<String, VarId>>,
    pub(crate) sealed_blocks: HashSet<BlockId>,
    pub(crate) global_strings: Vec<(String, String)>,
    pub(crate) variable_allocas: HashMap<String, VarId>,
    pub(crate) global_vars: HashSet<String>,
    pub(crate) global_types: HashMap<String, Type>,
    pub(crate) function_names: HashSet<String>,
    // Stack of (continue_target, break_target) for nested loops
    pub(crate) loop_context: Vec<(BlockId, BlockId)>,
    pub(crate) struct_defs: HashMap<String, model::StructDef>,
    pub(crate) union_defs: HashMap<String, model::UnionDef>,
    pub(crate) enum_constants: HashMap<String, i64>, // enum constant name => value
    pub(crate) typedefs: HashMap<String, Type>,
    pub(crate) current_switch_cases: Vec<(i64, BlockId)>, // (value, block)
    pub(crate) current_default: Option<BlockId>,
    pub(crate) break_targets: Vec<BlockId>,
    pub(crate) current_return_type: Option<Type>,
    // For goto/label support
    pub(crate) labels: HashMap<String, BlockId>,  // label name => block
    pub(crate) pending_gotos: Vec<(String, BlockId)>, // (label, goto_block) for forward gotos
    // Variable types for IR variables (used for float/int conversions)
    pub(crate) var_types: HashMap<VarId, Type>,
    pub(crate) param_indices: HashMap<String, usize>,
    // Cache for predecessor lookups
    pub(crate) pred_cache: HashMap<BlockId, Vec<BlockId>>,
    pub(crate) pred_cache_valid: bool,
    // Cache for type sizes (using string representation as key since Type doesn't implement Hash)
    pub(crate) type_size_cache: HashMap<String, i64>,
}

impl Lowerer {
    /// Create a new Lowerer instance
    pub fn new() -> Self {
        Self {
            next_var: 0,
            next_block: 0,
            current_def: HashMap::new(),
            symbol_table: HashMap::new(),
            variable_defs: HashMap::new(),
            blocks: Vec::new(),
            current_block: None,
            incomplete_phis: HashMap::new(),
            sealed_blocks: HashSet::new(),
            global_strings: Vec::new(),
            variable_allocas: HashMap::new(),
            global_vars: HashSet::new(),
            global_types: HashMap::new(),
            function_names: HashSet::new(),
            loop_context: Vec::new(),
            struct_defs: HashMap::new(),
            union_defs: HashMap::new(),
            enum_constants: HashMap::new(),
            typedefs: HashMap::new(),
            current_switch_cases: Vec::new(),
            current_default: None,
            break_targets: Vec::new(),
            labels: HashMap::new(),
            pending_gotos: Vec::new(),
            var_types: HashMap::new(),
            param_indices: HashMap::new(),
            current_return_type: None,
            pred_cache: HashMap::new(),
            pred_cache_valid: false,
            type_size_cache: HashMap::new(),
        }
    }

    /// Allocate a new variable ID
    pub(crate) fn new_var(&mut self) -> VarId {
        let id = self.next_var;
        self.next_var += 1;
        VarId(id)
    }

    /// Create a new basic block
    pub(crate) fn new_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        let block = BasicBlock {
            id: BlockId(id),
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
            is_label_target: false,
        };
        self.blocks.push(block);
        BlockId(id)
    }

    /// Add an instruction to the current block
    pub(crate) fn add_instruction(&mut self, instr: Instruction) {
        if let Some(bid) = self.current_block {
            self.blocks[bid.0].instructions.push(instr);
        }
    }

    /// Resolve a type that may contain `TypeofExpr` to a concrete type.
    pub(crate) fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::TypeofExpr(expr) => self.get_expr_type(expr),
            Type::Pointer(inner) => Type::Pointer(Box::new(self.resolve_type(inner))),
            Type::Array(inner, size) => Type::Array(Box::new(self.resolve_type(inner)), *size),
            other => other.clone(),
        }
    }

    /// Get the type of an expression
    pub(crate) fn get_expr_type(&self, expr: &AstExpr) -> Type {
        match expr {
            AstExpr::Constant(_) => Type::Int,
            AstExpr::FloatConstant(_) => Type::Double,  // Default float literals to double
            AstExpr::Variable(name) => {
                if let Some(ty) = self.symbol_table.get(name) {
                    ty.clone()
                } else if let Some(ty) = self.global_types.get(name) {
                    ty.clone()
                } else {
                    Type::Int // Default to int for undeclared, should be caught by semantic
                }
            }
            AstExpr::Binary { left, op, .. } => {
                if matches!(op, model::BinaryOp::Assign) {
                    self.get_expr_type(left)
                } else if matches!(op, model::BinaryOp::Less | model::BinaryOp::LessEqual | model::BinaryOp::Greater | model::BinaryOp::GreaterEqual | model::BinaryOp::EqualEqual | model::BinaryOp::NotEqual | model::BinaryOp::LogicalAnd | model::BinaryOp::LogicalOr) {
                    Type::Int
                } else {
                    self.get_expr_type(left)
                }
            }
            AstExpr::Unary { op, expr } => {
                match op {
                    model::UnaryOp::AddrOf => Type::Pointer(Box::new(self.get_expr_type(expr))),
                    model::UnaryOp::Deref => {
                        let ty = self.get_expr_type(expr);
                        if let Type::Pointer(inner) = ty {
                            *inner
                        } else if let Type::Array(inner, _) = ty {
                            *inner
                        } else {
                            Type::Int
                        }
                    }
                    _ => self.get_expr_type(expr),
                }
            }
            AstExpr::PostfixIncrement(expr) | AstExpr::PostfixDecrement(expr) 
            | AstExpr::PrefixIncrement(expr) | AstExpr::PrefixDecrement(expr) => {
                self.get_expr_type(expr)
            }
            AstExpr::Cast(ty, _) => ty.clone(),
            AstExpr::Member { expr, member } => {
                let ty = self.get_expr_type(expr);
                match ty {
                    Type::Struct(name) => {
                        if let Some(s_def) = self.struct_defs.get(&name) {
                            for field in &s_def.fields {
                                if &field.name == member {
                                    return field.field_type.clone();
                                }
                            }
                        }
                        Type::Int
                    }
                    Type::Union(name) => {
                        if let Some(u_def) = self.union_defs.get(&name) {
                                    for field in &u_def.fields {
                                if &field.name == member {
                                    return field.field_type.clone();
                                }
                            }
                        }
                        Type::Int
                    }
                    _ => Type::Int,
                }
            }
            AstExpr::PtrMember { expr, member } => {
                let ty = self.get_expr_type(expr);
                match ty {
                    Type::Pointer(inner) => {
                        match *inner {
                            Type::Struct(name) => {
                                if let Some(s_def) = self.struct_defs.get(&name) {
                                    for field in &s_def.fields {
                                        if &field.name == member {
                                            return field.field_type.clone();
                                        }
                                    }
                                }
                                Type::Int
                            }
                            Type::Union(name) => {
                                if let Some(u_def) = self.union_defs.get(&name) {
                                    for field in &u_def.fields {
                                        if &field.name == member {
                                            return field.field_type.clone();
                                        }
                                    }
                                }
                                Type::Int
                            }
                            _ => Type::Int,
                        }
                    }
                    _ => Type::Int,
                }
            }
            AstExpr::Index { array, .. } => {
                let ty = self.get_expr_type(array);
                match ty {
                    Type::Array(inner, _) => *inner,
                    Type::Pointer(inner) => *inner,
                    _ => Type::Int,
                }
            }
            AstExpr::Call { func: _, args:_ } => Type::Int, // Assume int return
            AstExpr::SizeOf(_) | AstExpr::SizeOfExpr(_) | AstExpr::AlignOf(_) => Type::Int,
            AstExpr::StringLiteral(_) => Type::Pointer(Box::new(Type::Char)),
            AstExpr::Conditional { then_expr, .. } => {
                // Ternary operator type is the type of the then/else branches
                // (In C, both branches should have compatible types)
                self.get_expr_type(then_expr)
            }
            AstExpr::CompoundLiteral { r#type, .. } => r#type.clone(),
            AstExpr::StmtExpr(stmts) => {
                // Statement expression type is the type of the last expr stmt
                if let Some(model::Stmt::Expr(expr)) = stmts.last() {
                    self.get_expr_type(expr)
                } else {
                    Type::Int
                }
            }
            AstExpr::Comma(exprs) => {
                // Comma expression type is the type of the last sub-expression
                if let Some(last) = exprs.last() {
                    self.get_expr_type(last)
                } else {
                    Type::Int
                }
            }
            AstExpr::InitList(_) => {
                // InitList type is context-dependent (resolved during lowering)
                Type::Int
            }
            AstExpr::BuiltinOffsetof { .. } => {
                // offsetof always returns an integer (size_t, effectively)
                Type::Long
            }
            AstExpr::Generic { controlling, associations } => {
                // Resolve to the matching association's expression type
                let ctrl_type = self.get_expr_type(controlling);
                for (assoc_type, expr) in associations {
                    match assoc_type {
                        Some(ty) if self.types_compatible(&ctrl_type, ty) => {
                            return self.get_expr_type(expr);
                        }
                        _ => {}
                    }
                }
                // Fall back to default
                for (assoc_type, expr) in associations {
                    if assoc_type.is_none() {
                        return self.get_expr_type(expr);
                    }
                }
                Type::Int
            }
        }
    }

    /// Calculate the size of a type in bytes
    /// (Implementation in type_utils.rs)

    /// Lower an entire AST program to IR
    pub fn lower_program(&mut self, ast: &AstProgram) -> Result<IRProgram, String> {
        self.global_vars.clear();
        self.function_names.clear();
        self.struct_defs.clear();
        self.union_defs.clear();
        self.enum_constants.clear();
        
        for s_def in &ast.structs {
            self.struct_defs.insert(s_def.name.clone(), s_def.clone());
        }
        
        for u_def in &ast.unions {
            self.union_defs.insert(u_def.name.clone(), u_def.clone());
        }
        
        // Register all enum constants
        for enum_def in &ast.enums {
            for (const_name, const_value) in &enum_def.constants {
                self.enum_constants.insert(const_name.clone(), *const_value);
            }
        }
        
        self.global_types.clear();
        for g in &ast.globals {
            self.global_vars.insert(g.name.clone());
            self.global_types.insert(g.name.clone(), g.r#type.clone());
        }
        // Add function names as globals (they can be used as function pointers)
        for f in &ast.functions {
            self.global_vars.insert(f.name.clone());
            self.function_names.insert(f.name.clone());
        }

        let mut functions = Vec::new();
        for f in &ast.functions {
            functions.push(self.lower_function(f)?);
        }
        Ok(IRProgram {
            functions,
            global_strings: self.global_strings.clone(),
            globals: ast.globals.clone(),
            structs: ast.structs.clone(),
            unions: ast.unions.clone(),
        })
    }

    /// Lower a single function to IR
    pub(crate) fn lower_function(&mut self, f: &AstFunction) -> Result<Function, String> {
        self.current_def.clear();
        self.symbol_table.clear();
        self.variable_defs.clear();
        self.blocks.clear();
        self.next_var = 0;
        self.next_block = 0;
        self.incomplete_phis.clear();
        self.sealed_blocks.clear();
        self.variable_allocas.clear();
        self.labels.clear();
        self.pending_gotos.clear();
        self.current_return_type = Some(f.return_type.clone());
        self.param_indices.clear();
        self.pred_cache.clear();
        self.pred_cache_valid = false;

        let entry_id = self.new_block();
        self.current_block = Some(entry_id);
        self.sealed_blocks.insert(entry_id);

        let mut params = Vec::new();
        for (i, (t, name)) in f.params.iter().enumerate() {
            let var = self.new_var();
            // Map parameter name to index
            self.param_indices.insert(name.clone(), i);

            // Create stack slot for parameter (to support address-of and mem2reg will optimize if not needed)
            let stack_slot = self.new_var();
            self.blocks[entry_id.0].instructions.push(Instruction::Alloca {
                dest: stack_slot,
                r#type: t.clone(),
            });
            self.variable_allocas.insert(name.clone(), stack_slot);
            self.var_types.insert(stack_slot, Type::Pointer(Box::new(t.clone())));
            
            // Store initial value
            self.blocks[entry_id.0].instructions.push(Instruction::Store {
                addr: Operand::Var(stack_slot),
                src: Operand::Var(var),
                value_type: t.clone(),
            });

            self.symbol_table.insert(name.clone(), t.clone());
            params.push((t.clone(), var));
        }

        self.lower_block(&f.body)?;
        
        // Check for unresolved gotos
        if !self.pending_gotos.is_empty() {
            let labels: Vec<String> = self.pending_gotos.iter().map(|(l, _)| l.clone()).collect();
            return Err(format!("Undefined labels: {:?}", labels));
        }
        
        // Ensure the last block has a return if it's void or just hanging
        if let Some(bid) = self.current_block {
             if matches!(self.blocks[bid.0].terminator, Terminator::Unreachable) {
                if f.return_type == Type::Void {
                    self.blocks[bid.0].terminator = Terminator::Ret(None);
                } else {
                    // Non-void function fell off the end — insert implicit return 0
                    // (matches GCC/Clang behavior for missing return in non-void functions)
                    self.blocks[bid.0].terminator = Terminator::Ret(Some(Operand::Constant(0)));
                }
             }
        }

        Ok(Function {
            name: f.name.clone(),
            return_type: f.return_type.clone(),
            params,
            blocks: self.blocks.clone(),
            entry_block: entry_id,
            var_types: self.var_types.clone(),
            attributes: f.attributes.clone(),
            is_static: f.is_static,
        })
    }

    /// Check if a name refers to a local variable
    pub(crate) fn is_local(&self, name: &str) -> bool {
        self.variable_defs.contains_key(name) || self.variable_allocas.contains_key(name)
    }

    /// Check if a name refers to a function
    pub(crate) fn is_function(&self, name: &str) -> bool {
        self.function_names.contains(name)
    }

    /// Get the type of an operand
    pub(crate) fn get_operand_type(&self, op: &crate::types::Operand) -> Result<Type, String> {
        match op {
            crate::types::Operand::Constant(_) => Ok(Type::Int),
            crate::types::Operand::FloatConstant(_) => Ok(Type::Float),
            crate::types::Operand::Var(v) => {
                // Check var_types if tracked
                if let Some(ty) = self.var_types.get(v) {
                    Ok(ty.clone())
                } else {
                    // Default to int if not tracked
                    Ok(Type::Int)
                }
            }
            crate::types::Operand::Global(name) => {
                // Globals are pointer types
                if let Some(ty) = self.symbol_table.get(name) {
                    Ok(Type::Pointer(Box::new(ty.clone())))
                } else {
                    Err(format!("Unknown global: {}", name))
                }
            }
        }
    }

}
