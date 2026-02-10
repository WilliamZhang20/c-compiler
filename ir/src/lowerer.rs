use model::{Type, Program as AstProgram, Function as AstFunction, Expr as AstExpr};
use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, BasicBlock, Function, IRProgram, Instruction, Terminator};

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
    // For goto/label support
    pub(crate) labels: HashMap<String, BlockId>,  // label name => block
    pub(crate) pending_gotos: Vec<(String, BlockId)>, // (label, goto_block) for forward gotos
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

    /// Get the type of an expression
    pub(crate) fn get_expr_type(&self, expr: &AstExpr) -> Type {
        match expr {
            AstExpr::Constant(_) => Type::Int,
            AstExpr::FloatConstant(_) => Type::Double,  // Default float literals to double
            AstExpr::Variable(name) => {
                if let Some(ty) = self.symbol_table.get(name) {
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
            AstExpr::SizeOf(_) | AstExpr::SizeOfExpr(_) => Type::Int,
            AstExpr::StringLiteral(_) => Type::Pointer(Box::new(Type::Char)),
            AstExpr::Conditional { then_expr, .. } => {
                // Ternary operator type is the type of the then/else branches
                // (In C, both branches should have compatible types)
                self.get_expr_type(then_expr)
            }
        }
    }

    /// Calculate the size of a type in bytes
    pub(crate) fn get_type_size(&self, ty: &Type) -> i64 {
        match ty {
            Type::Int | Type::UnsignedInt => 4,  // 32-bit int
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Long | Type::UnsignedLong => 8,  // 64-bit on x64
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,  // 32-bit float
            Type::Double => 8, // 64-bit double
            Type::Void => 0,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8, // Function pointers are 8 bytes
            Type::Array(base, size) => self.get_type_size(base) * (*size as i64),
            Type::Struct(name) => {
                if let Some(s_def) = self.struct_defs.get(name) {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    let mut size = 0;
                    
                    for field in &s_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        
                        // Align field if not packed
                        if !is_packed {
                            let alignment = self.get_alignment(&field.field_type);
                            // Align current size to field alignment
                            size = ((size + alignment - 1) / alignment) * alignment;
                        }
                        
                        size += field_size;
                    }
                    
                    // Add padding to make struct size a multiple of its alignment
                    if !is_packed {
                        let struct_alignment = self.get_alignment(ty);
                        size = ((size + struct_alignment - 1) / struct_alignment) * struct_alignment;
                    }
                    
                    size
                } else {
                    4 // fallback or error
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.union_defs.get(name) {
                    // Union size is the largest field
                    let mut max_size = 0;
                    for field in &u_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        if field_size > max_size {
                            max_size = field_size;
                        }
                    }
                    max_size
                } else {
                    4 // fallback
                }
            }
            Type::Typedef(name) => {
                if let Some(real_ty) = self.typedefs.get(name) {
                    self.get_type_size(real_ty)
                } else {
                    4
                }
            }
        }
    }

    /// Get the natural alignment of a type in bytes
    pub(crate) fn get_alignment(&self, ty: &Type) -> i64 {
        match ty {
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt => 4,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,
            Type::Double => 8,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8,
            Type::Array(base, _) => self.get_alignment(base),
            Type::Struct(name) => {
                if let Some(s_def) = self.struct_defs.get(name) {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    if is_packed {
                        return 1; // Packed structs have alignment 1
                    }
                    let mut max_alignment = 1;
                    for field in &s_def.fields {
                        let field_align = self.get_alignment(&field.field_type);
                        if field_align > max_alignment {
                            max_alignment = field_align;
                        }
                    }
                    max_alignment
                } else {
                    4
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.union_defs.get(name) {
                    let mut max_alignment = 1;
                    for field in &u_def.fields {
                        let field_align = self.get_alignment(&field.field_type);
                        if field_align > max_alignment {
                            max_alignment = field_align;
                        }
                    }
                    max_alignment
                } else {
                    4
                }
            }
            Type::Typedef(name) => {
                if let Some(real_ty) = self.typedefs.get(name) {
                    self.get_alignment(real_ty)
                } else {
                    4
                }
            }
            Type::Void => 1,
        }
    }

    /// Check if a type is a floating-point type
    pub(crate) fn is_float_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Float | Type::Double)
    }

    /// Get the byte offset and type of a struct/union member
    pub(crate) fn get_member_offset(&self, struct_or_union_name: &str, member_name: &str) -> (i64, Type) {
        // Check if it's a struct
        if let Some(s_def) = self.struct_defs.get(struct_or_union_name) {
            let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
            let mut offset = 0;
            
            for field in &s_def.fields {
                // Align the offset if not packed
                if !is_packed {
                    let alignment = self.get_alignment(&field.field_type);
                    // Round up to next aligned boundary
                    offset = ((offset + alignment - 1) / alignment) * alignment;
                }
                
                if &field.name == member_name {
                    return (offset, field.field_type.clone());
                }
                offset += self.get_type_size(&field.field_type);
            }
        }
        // Check if it's a union (all fields at offset 0)
        if let Some (u_def) = self.union_defs.get(struct_or_union_name) {
            for field in &u_def.fields {
                if &field.name == member_name {
                    return (0, field.field_type.clone());  // All union fields start at offset 0
                }
            }
        }
        (0, Type::Int)
    }

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
        
        for g in &ast.globals {
            self.global_vars.insert(g.name.clone());
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

        let entry_id = self.new_block();
        self.current_block = Some(entry_id);
        self.sealed_blocks.insert(entry_id);

        let mut params = Vec::new();
        for (t, name) in &f.params {
            let var = self.new_var();
            self.write_variable(name, entry_id, var);
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
                }
             }
        }

        Ok(Function {
            name: f.name.clone(),
            return_type: f.return_type.clone(),
            params,
            blocks: self.blocks.clone(),
            entry_block: entry_id,
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
}
