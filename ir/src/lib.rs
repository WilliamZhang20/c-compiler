/*
TODO: Modularize
*/
use model::{BinaryOp, UnaryOp, Type, Program as AstProgram, Function as AstFunction, Stmt as AstStmt, Expr as AstExpr, Block as AstBlock, GlobalVar as AstGlobalVar};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VarId(pub usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone)]
pub enum Operand {
    Constant(i64),
    Var(VarId),
    Global(String),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Binary {
        dest: VarId,
        op: BinaryOp,
        left: Operand,
        right: Operand,
    },
    Unary {
        dest: VarId,
        op: UnaryOp,
        src: Operand,
    },
    Phi {
        dest: VarId,
        // (BlockId where the value comes from, Operand)
        preds: Vec<(BlockId, VarId)>,
    },
    Copy {
        dest: VarId,
        src: Operand,
    },
    Alloca {
        dest: VarId,
        r#type: Type,
    },
    Load {
        dest: VarId,
        addr: Operand,
    },
    Store {
        addr: Operand,
        src: Operand,
    },
    GetElementPtr {
        dest: VarId,
        base: Operand,
        index: Operand,
        element_type: Type,
    },
    Call {
        dest: Option<VarId>,
        name: String,
        args: Vec<Operand>,
    },
    IndirectCall {
        dest: Option<VarId>,
        func_ptr: Operand,
        args: Vec<Operand>,
    },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Br(BlockId),
    CondBr {
        cond: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    Ret(Option<Operand>),
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(Type, VarId)>,
    pub blocks: Vec<BasicBlock>,
    pub entry_block: BlockId,
}

#[derive(Debug, Clone)]
pub struct IRProgram {
    pub functions: Vec<Function>,
    pub global_strings: Vec<(String, String)>, // (label, content)
    pub globals: Vec<AstGlobalVar>,
    pub structs: Vec<model::StructDef>,
}

pub struct Lowerer {
    next_var: usize,
    next_block: usize,
    current_def: HashMap<String, VarId>,
    symbol_table: HashMap<String, Type>,
    variable_defs: HashMap<String, HashMap<BlockId, VarId>>,
    blocks: Vec<BasicBlock>,
    current_block: Option<BlockId>,
    incomplete_phis: HashMap<BlockId, HashMap<String, VarId>>,
    sealed_blocks: HashSet<BlockId>,
    global_strings: Vec<(String, String)>,
    variable_allocas: HashMap<String, VarId>,
    global_vars: HashSet<String>,
    function_names: HashSet<String>,
    // Stack of (continue_target, break_target) for nested loops
    loop_context: Vec<(BlockId, BlockId)>,
    struct_defs: HashMap<String, model::StructDef>,
    typedefs: HashMap<String, Type>,
    current_switch_cases: Vec<(i64, BlockId)>, // (value, block)
    current_default: Option<BlockId>,
    break_targets: Vec<BlockId>,
}

impl Lowerer {
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
            typedefs: HashMap::new(),
            current_switch_cases: Vec::new(),
            current_default: None,
            break_targets: Vec::new(),
        }
    }

    fn new_var(&mut self) -> VarId {
        let id = self.next_var;
        self.next_var += 1;
        VarId(id)
    }

    fn new_block(&mut self) -> BlockId {
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

    fn add_instruction(&mut self, instr: Instruction) {
        if let Some(bid) = self.current_block {
            self.blocks[bid.0].instructions.push(instr);
        }
    }

    fn get_expr_type(&self, expr: &AstExpr) -> Type {
        match expr {
            AstExpr::Constant(_) => Type::Int,
            AstExpr::Variable(name) => {
                if let Some(ty) = self.symbol_table.get(name) {
                    ty.clone()
                } else {
                    Type::Int // Default to int for undeclared, should be caught by semantic
                }
            }
            AstExpr::Binary { left, op, .. } => {
                if matches!(op, BinaryOp::Assign) {
                    self.get_expr_type(left)
                } else if matches!(op, BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::EqualEqual | BinaryOp::NotEqual | BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
                    Type::Int
                } else {
                    self.get_expr_type(left)
                }
            }
            AstExpr::Unary { op, expr } => {
                match op {
                    UnaryOp::AddrOf => Type::Pointer(Box::new(self.get_expr_type(expr))),
                    UnaryOp::Deref => {
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
            AstExpr::Cast(ty, _) => ty.clone(),
            AstExpr::Member { expr, member } => {
                let ty = self.get_expr_type(expr);
                match ty {
                    Type::Struct(name) => {
                        if let Some(s_def) = self.struct_defs.get(&name) {
                            for (f_ty, f_name) in &s_def.fields {
                                if f_name == member {
                                    return f_ty.clone();
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
                        if let Type::Struct(name) = *inner {
                            if let Some(s_def) = self.struct_defs.get(&name) {
                                for (f_ty, f_name) in &s_def.fields {
                                    if f_name == member {
                                        return f_ty.clone();
                                    }
                                }
                            }
                        }
                        Type::Int
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
            AstExpr::Call { func: _, args: _ } => Type::Int, // Assume int return
            AstExpr::SizeOf(_) | AstExpr::SizeOfExpr(_) => Type::Int,
            AstExpr::StringLiteral(_) => Type::Pointer(Box::new(Type::Char)),
        }
    }

    fn get_type_size(&self, ty: &Type) -> i64 {
        match ty {
            Type::Int => 4,
            Type::Char => 1,
            Type::Void => 0,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8, // Function pointers are 8 bytes
            Type::Array(base, size) => self.get_type_size(base) * (*size as i64),
            Type::Struct(name) => {
                if let Some(s_def) = self.struct_defs.get(name) {
                    let mut size = 0;
                    for (f_ty, _) in &s_def.fields {
                        // Very simple alignment: next available byte
                        size += self.get_type_size(f_ty);
                    }
                    size
                } else {
                    4 // fallback or error
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

    fn get_member_offset(&self, struct_name: &str, member_name: &str) -> (i64, Type) {
        if let Some(s_def) = self.struct_defs.get(struct_name) {
            let mut offset = 0;
            for (f_ty, f_name) in &s_def.fields {
                if f_name == member_name {
                    return (offset, f_ty.clone());
                }
                offset += self.get_type_size(f_ty);
            }
        }
        (0, Type::Int)
    }
    pub fn lower_program(&mut self, ast: &AstProgram) -> Result<IRProgram, String> {
        self.global_vars.clear();
        self.function_names.clear();
        self.struct_defs.clear();
        for s_def in &ast.structs {
            self.struct_defs.insert(s_def.name.clone(), s_def.clone());
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
        })
    }

    fn lower_function(&mut self, f: &AstFunction) -> Result<Function, String> {
        self.current_def.clear();
        self.symbol_table.clear();
        self.variable_defs.clear();
        self.blocks.clear();
        self.next_var = 0;
        self.next_block = 0;
        self.incomplete_phis.clear();
        self.sealed_blocks.clear();
        self.variable_allocas.clear();

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

    fn lower_block(&mut self, block: &AstBlock) -> Result<(), String> {
        for stmt in &block.statements {
            self.lower_stmt(stmt)?;
        }
        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &AstStmt) -> Result<(), String> {
        match stmt {
            AstStmt::Return(expr) => {
                let val = if let Some(e) = expr {
                    Some(self.lower_expr(e)?)
                } else {
                    None
                };
                let bid = self.current_block.ok_or("Return outside of block")?;
                self.blocks[bid.0].terminator = Terminator::Ret(val);
                self.current_block = None; // Dead code after return
            }
            AstStmt::Declaration { r#type, name, init } => {
                self.symbol_table.insert(name.clone(), r#type.clone());
                let bid = self.current_block.ok_or("Declaration outside of block")?;
                
                if matches!(r#type, Type::Array(..)) {
                    let var = self.new_var();
                    self.blocks[bid.0].instructions.push(Instruction::Alloca {
                        dest: var,
                        r#type: r#type.clone(),
                    });
                    self.write_variable(name, bid, var);
                    self.variable_allocas.insert(name.clone(), var);
                } else {
                    // Alloca for all scalars too to support & operator
                    let alloca_var = self.new_var();
                    self.blocks[bid.0].instructions.push(Instruction::Alloca {
                        dest: alloca_var,
                        r#type: r#type.clone(),
                    });
                    self.variable_allocas.insert(name.clone(), alloca_var);

                    if let Some(e) = init {
                        let val = self.lower_expr(e)?;
                        self.blocks[bid.0].instructions.push(Instruction::Store {
                            addr: Operand::Var(alloca_var),
                            src: val.clone(),
                        });
                        
                        let var = match val {
                            Operand::Var(v) => v,
                            Operand::Constant(_) | Operand::Global(_) => {
                                let v = self.new_var();
                                self.blocks[bid.0].instructions.push(Instruction::Copy {
                                    dest: v,
                                    src: val,
                                });
                                v
                            }
                        };
                        self.write_variable(name, bid, var);
                    }
                }
            }
            AstStmt::Expr(e) => {
                self.lower_expr(e)?;
            }
            AstStmt::Block(b) => {
                self.lower_block(b)?;
            }
            AstStmt::If { cond, then_branch, else_branch } => {
                let cond_val = self.lower_expr(cond)?;
                let bid = self.current_block.ok_or("If outside of block")?;

                let then_id = self.new_block();
                let else_id = self.new_block();
                let merge_id = self.new_block();

                self.blocks[bid.0].terminator = Terminator::CondBr {
                    cond: cond_val,
                    then_block: then_id,
                    else_block: else_id,
                };

                // Lower Then
                self.sealed_blocks.insert(then_id);
                self.current_block = Some(then_id);
                self.lower_stmt(then_branch)?;
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(merge_id);
                }
                let then_end = self.current_block;

                // Lower Else
                self.sealed_blocks.insert(else_id);
                self.current_block = Some(else_id);
                if let Some(eb) = else_branch {
                    self.lower_stmt(eb)?;
                }
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(merge_id);
                }
                let else_end = self.current_block;

                // Merge
                self.sealed_blocks.insert(merge_id);
                if then_end.is_some() || else_end.is_some() {
                    self.current_block = Some(merge_id);
                    // Resolve incomplete Phis if any (none expected here as all preds were sealed)
                } else {
                    self.current_block = None;
                }
            }
            AstStmt::While { cond, body } => {
                let header_id = self.new_block();
                let body_id = self.new_block();
                let exit_id = self.new_block();

                let bid = self.current_block.ok_or("While outside of block")?;
                self.blocks[bid.0].terminator = Terminator::Br(header_id);

                self.current_block = Some(header_id);
                let cond_val = self.lower_expr(cond)?;
                self.blocks[header_id.0].terminator = Terminator::CondBr {
                    cond: cond_val,
                    then_block: body_id,
                    else_block: exit_id,
                };

                self.sealed_blocks.insert(body_id);
                self.current_block = Some(body_id);
                self.loop_context.push((header_id, exit_id));
                self.lower_stmt(body)?;
                self.loop_context.pop();
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(header_id);
                }

                self.seal_block(header_id);
                self.seal_block(exit_id);
                
                self.current_block = Some(exit_id);
            }
            AstStmt::DoWhile { body, cond } => {
                let body_id = self.new_block();
                let latch_id = self.new_block();
                let exit_id = self.new_block();

                let bid = self.current_block.ok_or("Do-while outside of block")?;
                self.blocks[bid.0].terminator = Terminator::Br(body_id);

                self.current_block = Some(body_id);
                self.loop_context.push((latch_id, exit_id));
                self.lower_stmt(body)?;
                self.loop_context.pop();
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(latch_id);
                }

                self.sealed_blocks.insert(latch_id);
                self.current_block = Some(latch_id);
                let cond_val = self.lower_expr(cond)?;
                self.blocks[latch_id.0].terminator = Terminator::CondBr {
                    cond: cond_val,
                    then_block: body_id,
                    else_block: exit_id,
                };

                self.seal_block(body_id);
                self.seal_block(exit_id);

                self.current_block = Some(exit_id);
            }
            AstStmt::For { init, cond, post, body } => {
                if let Some(stmt) = init {
                    self.lower_stmt(stmt)?;
                }

                let header_id = self.new_block();
                let body_id = self.new_block();
                let post_id = self.new_block();
                let exit_id = self.new_block();

                // Branch from current block (after init) to header
                if let Some(bid) = self.current_block {
                    self.blocks[bid.0].terminator = Terminator::Br(header_id);
                }

                self.current_block = Some(header_id);
                if let Some(c) = cond {
                    let cond_val = self.lower_expr(c)?;
                    self.blocks[header_id.0].terminator = Terminator::CondBr {
                        cond: cond_val,
                        then_block: body_id,
                        else_block: exit_id,
                    };
                } else {
                    self.blocks[header_id.0].terminator = Terminator::Br(body_id);
                }

                self.sealed_blocks.insert(body_id);
                self.current_block = Some(body_id);
                self.loop_context.push((post_id, exit_id));
                self.lower_stmt(body)?;
                self.loop_context.pop();
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(post_id);
                }

                self.sealed_blocks.insert(post_id);
                self.current_block = Some(post_id);
                if let Some(p) = post {
                    self.lower_expr(p)?;
                }
                self.blocks[post_id.0].terminator = Terminator::Br(header_id);

                self.seal_block(header_id);
                self.seal_block(exit_id);

                self.current_block = Some(exit_id);
            }

            AstStmt::Continue => {
                if let Some((continue_target, _)) = self.loop_context.last() {
                    let bid = self.current_block.ok_or("Continue outside of block")?;
                    self.blocks[bid.0].terminator = Terminator::Br(*continue_target);
                    self.current_block = None;
                } else {
                    return Err("Continue outside of loop".to_string());
                }
            }
            AstStmt::Break => {
                if let Some((_, break_target)) = self.loop_context.last() {
                     let bid = self.current_block.ok_or("Break outside of block")?;
                     self.blocks[bid.0].terminator = Terminator::Br(*break_target);
                     self.current_block = None;
                } else if let Some(break_target) = self.break_targets.last() {
                     let bid = self.current_block.ok_or("Break outside of block")?;
                     self.blocks[bid.0].terminator = Terminator::Br(*break_target);
                     self.current_block = None;
                } else {
                    return Err("Break not in loop or switch".to_string());
                }
            }
            AstStmt::Switch { cond, body } => {
                let cond_val = self.lower_expr(cond)?;
                let head = self.new_block();
                let end = self.new_block();
                
                let bid = self.current_block.ok_or("Switch outside block")?;
                self.blocks[bid.0].terminator = Terminator::Br(head);
                self.seal_block(head);
                
                self.break_targets.push(end);
                let old_cases = std::mem::take(&mut self.current_switch_cases);
                let old_default = self.current_default.take();
                
                // Lower body - this will register cases in self.current_switch_cases
                self.current_block = Some(self.new_block()); // Start of body
                let body_start = self.current_block.unwrap();
                self.seal_block(body_start);
                self.lower_stmt(body)?;
                
                let cases = std::mem::take(&mut self.current_switch_cases);
                let default = self.current_default.take();
                self.break_targets.pop();

                // Finish the body if it's still open
                if let Some(bid) = self.current_block {
                    self.blocks[bid.0].terminator = Terminator::Br(end);
                }

                // Now fill the head with comparisons
                self.current_block = Some(head);
                let mut current_head = head;
                for (val, block) in cases {
                    let next_head = self.new_block();
                    let cond_var = self.new_var();
                    self.add_instruction(Instruction::Binary {
                        dest: cond_var,
                        op: model::BinaryOp::EqualEqual,
                        left: cond_val.clone(),
                        right: Operand::Constant(val),
                    });
                    self.blocks[current_head.0].terminator = Terminator::CondBr {
                        cond: Operand::Var(cond_var),
                        then_block: block,
                        else_block: next_head,
                    };
                    self.seal_block(next_head);
                    current_head = next_head;
                    self.current_block = Some(next_head);
                }
                
                let default_target = default.unwrap_or(end);
                self.blocks[current_head.0].terminator = Terminator::Br(default_target);
                
                self.current_block = Some(end);
                self.seal_block(end);
                
                // Restore old context
                self.current_switch_cases = old_cases;
                self.current_default = old_default;
            }
            AstStmt::Case(expr) => {
                if let AstExpr::Constant(val) = expr {
                    let case_block = self.new_block();
                    if let Some(bid) = self.current_block {
                        self.blocks[bid.0].terminator = Terminator::Br(case_block);
                    }
                    self.current_switch_cases.push((*val, case_block));
                    self.seal_block(case_block);
                    self.current_block = Some(case_block);
                } else {
                    return Err("Case label must be a constant".to_string());
                }
            }
            AstStmt::Default => {
                let default_block = self.new_block();
                if let Some(bid) = self.current_block {
                    self.blocks[bid.0].terminator = Terminator::Br(default_block);
                }
                self.current_default = Some(default_block);
                self.seal_block(default_block);
                self.current_block = Some(default_block);
            }
        }
        Ok(())
    }

    fn seal_block(&mut self, block: BlockId) {
        if self.sealed_blocks.contains(&block) { return; }
        let phis = self.incomplete_phis.remove(&block).unwrap_or_default();
        for (name, phi_var) in phis {
            self.add_phi_operands(&name, block, phi_var);
        }
        self.sealed_blocks.insert(block);
    }

    fn lower_expr(&mut self, expr: &AstExpr) -> Result<Operand, String> {
        match expr {
            AstExpr::Constant(c) => Ok(Operand::Constant(*c)),
            AstExpr::Binary { left, op, right } => {
                if *op == BinaryOp::Assign {
                    let val = self.lower_expr(right)?;
                    let addr = self.lower_to_addr(left)?;
                    self.add_instruction(Instruction::Store {
                        addr: Operand::Var(addr),
                        src: val.clone(),
                    });
                    return Ok(val);
                }
                let l_ty = self.get_expr_type(left);
                let r_ty = self.get_expr_type(right);

                let mut l_val = self.lower_expr(left)?;
                let mut r_val = self.lower_expr(right)?;

                if *op == BinaryOp::Add || *op == BinaryOp::Sub {
                    if let Type::Pointer(inner) = l_ty {
                        let size = self.get_type_size(&inner);
                        if size > 1 {
                            let scaled_r = self.new_var();
                            self.add_instruction(Instruction::Binary {
                                dest: scaled_r,
                                op: BinaryOp::Mul,
                                left: r_val,
                                right: Operand::Constant(size),
                            });
                            r_val = Operand::Var(scaled_r);
                        }
                    } else if let Type::Array(inner, _) = l_ty {
                        let size = self.get_type_size(&inner);
                        if size > 1 {
                             let scaled_r = self.new_var();
                             self.add_instruction(Instruction::Binary {
                                 dest: scaled_r,
                                 op: BinaryOp::Mul,
                                 left: r_val,
                                 right: Operand::Constant(size),
                             });
                             r_val = Operand::Var(scaled_r);
                        }
                    } else if *op == BinaryOp::Add {
                        if let Type::Pointer(inner) = r_ty {
                            let size = self.get_type_size(&inner);
                             if size > 1 {
                                let scaled_l = self.new_var();
                                self.add_instruction(Instruction::Binary {
                                    dest: scaled_l,
                                    op: BinaryOp::Mul,
                                    left: l_val,
                                    right: Operand::Constant(size),
                                });
                                l_val = Operand::Var(scaled_l);
                            }
                        }
                    }
                }

                let dest = self.new_var();
                self.add_instruction(Instruction::Binary {
                    dest,
                    op: op.clone(),
                    left: l_val,
                    right: r_val,
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Unary { op, expr: inner } if *op == UnaryOp::AddrOf => {
                let addr = self.lower_to_addr(inner)?;
                Ok(Operand::Var(addr))
            }
            AstExpr::Variable(name) if self.is_local(name) && !self.variable_allocas.contains_key(name) => {
                let bid = self.current_block.ok_or("Variable access outside block")?;
                Ok(Operand::Var(self.read_variable(name, bid)))
            }
            AstExpr::Variable(name) if self.is_function(name) => {
                // Function names evaluate to their address (function pointer)
                let dest = self.new_var();
                self.add_instruction(Instruction::Copy {
                    dest,
                    src: Operand::Global(name.clone()),
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Variable(_) | AstExpr::Index { .. } | AstExpr::Member { .. } | AstExpr::PtrMember { .. } | AstExpr::Unary { op: UnaryOp::Deref, .. } => {
                let addr = self.lower_to_addr(expr)?;
                let dest = self.new_var();
                self.add_instruction(Instruction::Load {
                    dest,
                    addr: Operand::Var(addr),
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Unary { op, expr } => {
                let val = self.lower_expr(expr)?;
                let dest = self.new_var();
                self.add_instruction(Instruction::Unary {
                    dest,
                    op: op.clone(),
                    src: val,
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::StringLiteral(s) => {
                let label = format!("str_{}", self.global_strings.len());
                self.global_strings.push((label.clone(), s.clone()));
                Ok(Operand::Global(label))
            }
            AstExpr::Call { func, args } => {
                let mut ir_args = Vec::new();
                for arg in args {
                    ir_args.push(self.lower_expr(arg)?);
                }
                let bid = self.current_block.ok_or("Call outside block")?;
                let dest = self.new_var();
                
                // Check if it's a direct call (function name) or indirect call (function pointer variable)
                let is_direct_call = if let AstExpr::Variable(name) = func.as_ref() {
                    self.is_function(name)
                } else {
                    false
                };
                
                if is_direct_call {
                    // Direct call to a function
                    if let AstExpr::Variable(name) = func.as_ref() {
                        self.blocks[bid.0].instructions.push(Instruction::Call {
                            dest: Some(dest),
                            name: name.clone(),
                            args: ir_args,
                        });
                    }
                } else {
                    // Indirect call through function pointer
                    let func_ptr = self.lower_expr(func)?;
                    self.blocks[bid.0].instructions.push(Instruction::IndirectCall {
                        dest: Some(dest),
                        func_ptr,
                        args: ir_args,
                    });
                }
                Ok(Operand::Var(dest))
            }
            AstExpr::SizeOf(ty) => {
                Ok(Operand::Constant(self.get_type_size(ty)))
            }
            AstExpr::SizeOfExpr(_expr) => {
                Ok(Operand::Constant(8)) 
            }
            AstExpr::Cast(_ty, expr) => {
                self.lower_expr(expr)
            }
        }
    }

    fn lower_to_addr(&mut self, expr: &AstExpr) -> Result<VarId, String> {
        let bid = self.current_block.ok_or("Address calculation outside block")?;
        match expr {
            AstExpr::Variable(name) => {
                if let Some(addr) = self.variable_allocas.get(name) {
                    Ok(*addr)
                } else if self.global_vars.contains(name) {
                    let dest = self.new_var();
                    self.blocks[bid.0].instructions.push(Instruction::Copy {
                        dest,
                        src: Operand::Global(name.clone()),
                    });
                    Ok(dest)
                } else {
                    Err(format!("Undefined variable {}", name))
                }
            }
            AstExpr::Index { array, index } => {
                let base_addr = self.lower_to_addr(array)?;
                let index_val = self.lower_expr(index)?;
                let dest = self.new_var();
                let array_type = self.get_expr_type(array);
                let element_type = match array_type {
                    Type::Array(inner, _) => *inner,
                    Type::Pointer(inner) => *inner,
                    _ => Type::Int, // fallback
                };
                self.blocks[bid.0].instructions.push(Instruction::GetElementPtr {
                    dest,
                    base: Operand::Var(base_addr),
                    index: index_val,
                    element_type,
                });
                Ok(dest)
            }
            AstExpr::Unary { op: UnaryOp::Deref, expr } => {
                let addr_op = self.lower_expr(expr)?;
                match addr_op {
                    Operand::Var(v) => Ok(v),
                    _ => Err("Dereference operand must be in a variable".to_string()),
                }
            }
            AstExpr::Member { expr, member } => {
                let base_addr = self.lower_to_addr(expr)?;
                // Get the struct type from the expression
                let expr_type = self.get_expr_type(expr);
                let struct_name = match &expr_type {
                    Type::Struct(name) => name.clone(),
                    _ => return Err(format!("Member access on non-struct type {:?}", expr_type)),
                };
                let (offset, _) = self.get_member_offset(&struct_name, member); 
                let dest = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::GetElementPtr {
                    dest,
                    base: Operand::Var(base_addr),
                    index: Operand::Constant(offset),
                    element_type: Type::Char, // byte offset for struct members
                });
                Ok(dest)
            }
            AstExpr::PtrMember { expr, member } => {
                let addr_op = self.lower_expr(expr)?;
                let base_addr = match addr_op {
                    Operand::Var(v) => v,
                    _ => return Err("-> operand must be in a variable".to_string()),
                };
                // Get the struct type from the pointer
                let expr_type = self.get_expr_type(expr);
                let struct_name = match &expr_type {
                    Type::Pointer(inner) => {
                        match &**inner {
                            Type::Struct(name) => name.clone(),
                            _ => return Err(format!("Pointer member access on non-struct pointer {:?}", expr_type)),
                        }
                    }
                    _ => return Err(format!("-> operator on non-pointer type {:?}", expr_type)),
                };
                let (offset, _) = self.get_member_offset(&struct_name, member);
                let dest = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::GetElementPtr {
                    dest,
                    base: Operand::Var(base_addr),
                    index: Operand::Constant(offset),
                    element_type: Type::Char, // byte offset for struct members
                });
                Ok(dest)
            }
            _ => Err("Expression is not an l-value".to_string()),
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.variable_defs.contains_key(name) || self.variable_allocas.contains_key(name)
    }

    fn is_function(&self, name: &str) -> bool {
        // A name is a function if it's in function_names
        self.function_names.contains(name)
    }
    
    fn is_global_var(&self, name: &str) -> bool {
        // A name is a global variable if it's in global_vars but not in function_names
        self.global_vars.contains(name) && !self.function_names.contains(name)
    }

    fn write_variable(&mut self, name: &str, block: BlockId, value: VarId) {
        self.variable_defs.entry(name.to_string())
            .or_insert_with(HashMap::new)
            .insert(block, value);
    }

    fn read_variable(&mut self, name: &str, block: BlockId) -> VarId {
        if let Some(defs) = self.variable_defs.get(name) {
            if let Some(var) = defs.get(&block) {
                return *var;
            }
        }
        self.read_variable_recursive(name, block)
    }

    fn read_variable_recursive(&mut self, name: &str, block: BlockId) -> VarId {
        let mut val;
        if !self.sealed_blocks.contains(&block) {
            // Incomplete Phi
            val = self.new_var();
            self.incomplete_phis.entry(block)
                .or_insert_with(HashMap::new)
                .insert(name.to_string(), val);
        } else {
            let preds = self.get_predecessors(block);
            if preds.len() == 1 {
                val = self.read_variable(name, preds[0]);
            } else {
                val = self.new_var();
                self.write_variable(name, block, val);
                val = self.add_phi_operands(name, block, val);
            }
        }
        self.write_variable(name, block, val);
        val
    }

    fn add_phi_operands(&mut self, name: &str, block: BlockId, phi_var: VarId) -> VarId {
        let preds = self.get_predecessors(block);
        let mut phi_preds = Vec::new();
        for pred in preds {
            let val = self.read_variable(name, pred);
            phi_preds.push((pred, val));
        }
        // Actually insert the Phi instruction at the beginning of the block
        self.blocks[block.0].instructions.insert(0, Instruction::Phi {
            dest: phi_var,
            preds: phi_preds,
        });
        // Trivial phi elimination could go here
        phi_var
    }

    fn get_predecessors(&self, block: BlockId) -> Vec<BlockId> {
        let mut preds = Vec::new();
        for b in &self.blocks {
            match &b.terminator {
                Terminator::Br(id) if *id == block => preds.push(b.id),
                Terminator::CondBr { then_block, else_block, .. } => {
                    if *then_block == block { preds.push(b.id); }
                    if *else_block == block { preds.push(b.id); }
                }
                _ => {}
            }
        }
        preds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lex;
    use parser::parse_tokens;

    #[test]
    fn test_lower_simple_arithmetic() {
        let src = "int main() { int a = 1; int b = 2; return a + b; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();
        
        assert_eq!(ir.functions.len(), 1);
        let f = &ir.functions[0];
        assert_eq!(f.name, "main");
        
        // entry block should have 2 copies and 1 binary op and a return
        let entry = &f.blocks[0];
        assert!(matches!(entry.terminator, Terminator::Ret(Some(Operand::Var(_)))));
    }

    #[test]
    fn test_lower_globals() {
        let src = "int g = 10; int main() { return g; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();
        
        assert_eq!(ir.functions.len(), 1);
        assert_eq!(ir.globals.len(), 1);
        
        let f = &ir.functions[0];
        // Should have a Load from Global
        let load = f.blocks[0].instructions.iter().find(|i| matches!(i, Instruction::Load { addr: Operand::Global(_), .. }));
        assert!(load.is_some());
    }
}
