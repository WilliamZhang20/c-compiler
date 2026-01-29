use model::{BinaryOp, UnaryOp, Type, Program as AstProgram, Function as AstFunction, Stmt as AstStmt, Expr as AstExpr, Block as AstBlock};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VarId(pub usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone)]
pub enum Operand {
    Constant(i64),
    Var(VarId),
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
}

pub struct Lowerer {
    next_var: usize,
    next_block: usize,
    // variable name -> latest SSA variable id
    current_def: HashMap<String, VarId>,
    // variable name -> type
    symbol_table: HashMap<String, Type>,
    // variable name -> BlockId -> VarId
    variable_defs: HashMap<String, HashMap<BlockId, VarId>>,
    blocks: Vec<BasicBlock>,
    current_block: Option<BlockId>,
    // Incomplete Phis: BlockId -> Variable Name -> VarId (the Phi dest)
    incomplete_phis: HashMap<BlockId, HashMap<String, VarId>>,
    sealed_blocks: HashSet<BlockId>,
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
        BlockId(id)
    }

    pub fn lower_program(&mut self, ast: &AstProgram) -> Result<IRProgram, String> {
        let mut functions = Vec::new();
        for f in &ast.functions {
            functions.push(self.lower_function(f)?);
        }
        Ok(IRProgram { functions })
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

        let entry_id = self.new_block();
        self.current_block = Some(entry_id);
        self.sealed_blocks.insert(entry_id);
        
        // Prepare blocks vector (stubs)
        self.blocks.push(BasicBlock {
            id: entry_id,
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
        });

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
                if self.symbol_table.contains_key(name) {
                    return Err(format!("Redeclaration of variable {}", name));
                }
                self.symbol_table.insert(name.clone(), r#type.clone());
                if let Some(e) = init {
                    let val = self.lower_expr(e)?;
                    let bid = self.current_block.ok_or("Declaration outside of block")?;
                    let var = match val {
                        Operand::Var(v) => v,
                        Operand::Constant(_) => {
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

                self.blocks.push(BasicBlock { id: then_id, instructions: Vec::new(), terminator: Terminator::Unreachable });
                self.blocks.push(BasicBlock { id: else_id, instructions: Vec::new(), terminator: Terminator::Unreachable });
                self.blocks.push(BasicBlock { id: merge_id, instructions: Vec::new(), terminator: Terminator::Unreachable });

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

                self.blocks.push(BasicBlock { id: header_id, instructions: Vec::new(), terminator: Terminator::Unreachable });
                self.blocks.push(BasicBlock { id: body_id, instructions: Vec::new(), terminator: Terminator::Unreachable });
                self.blocks.push(BasicBlock { id: exit_id, instructions: Vec::new(), terminator: Terminator::Unreachable });

                // Header is NOT sealed yet (loop back edge)
                self.current_block = Some(header_id);
                let cond_val = self.lower_expr(cond)?;
                self.blocks[header_id.0].terminator = Terminator::CondBr {
                    cond: cond_val,
                    then_block: body_id,
                    else_block: exit_id,
                };

                // Body
                self.sealed_blocks.insert(body_id);
                self.current_block = Some(body_id);
                self.lower_stmt(body)?;
                if let Some(curr) = self.current_block {
                    self.blocks[curr.0].terminator = Terminator::Br(header_id);
                }

                // Seal header and exit
                self.seal_block(header_id);
                self.seal_block(exit_id);
                
                self.current_block = Some(exit_id);
            }
            _ => return Err("Stmt not implemented in lowerer yet".to_string()),
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
            AstExpr::Variable(name) => {
                let bid = self.current_block.ok_or("Variable access outside block")?;
                Ok(Operand::Var(self.read_variable(name, bid)))
            }
            AstExpr::Binary { left, op, right } => {
                if *op == BinaryOp::Assign {
                    if let AstExpr::Variable(name) = &**left {
                        let val = self.lower_expr(right)?;
                        let bid = self.current_block.ok_or("Assignment outside block")?;
                        let var = match val {
                            Operand::Var(v) => v,
                            Operand::Constant(_) => {
                                let v = self.new_var();
                                self.blocks[bid.0].instructions.push(Instruction::Copy { dest: v, src: val });
                                v
                            }
                        };
                        self.write_variable(name, bid, var);
                        return Ok(Operand::Var(var));
                    }
                    return Err("LHS of assignment must be variable".to_string());
                }
                let l_val = self.lower_expr(left)?;
                let r_val = self.lower_expr(right)?;
                let bid = self.current_block.ok_or("Binary op outside block")?;
                let dest = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::Binary {
                    dest,
                    op: op.clone(),
                    left: l_val,
                    right: r_val,
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Unary { op, expr } => {
                let val = self.lower_expr(expr)?;
                let bid = self.current_block.ok_or("Unary op outside block")?;
                let dest = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::Unary {
                    dest,
                    op: op.clone(),
                    src: val,
                });
                Ok(Operand::Var(dest))
            }
        }
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
    fn test_lower_if_ssa() {
        let src = "int main() { int x = 1; if (x) { x = 2; } else { x = 3; } return x; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();

        let f = &ir.functions[0];
        // Total blocks: Entry, Then, Else, Merge
        assert_eq!(f.blocks.len(), 4);
        
        let merge_block = &f.blocks[3];
        // The return should use a Φ variable
        if let Terminator::Ret(Some(Operand::Var(v))) = merge_block.terminator {
            // Check if there's a Phi instruction defining this var
            let phi_exists = merge_block.instructions.iter().any(|inst| {
                if let Instruction::Phi { dest, .. } = inst {
                    *dest == v
                } else {
                    false
                }
            });
            assert!(phi_exists, "Merge block should have a Phi for x");
        } else {
            panic!("Expected return with variable from merge block");
        }
    }

    #[test]
    fn test_lower_while_ssa() {
        let src = "int main() { int x = 0; while (x < 10) { x = x + 1; } return x; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();

        let f = &ir.functions[0];
        // Entry, Header, Body, Exit
        assert_eq!(f.blocks.len(), 4);
        
        let header_block = &f.blocks[1];
        // Header should have a Phi for x because it's redefined in the body
        let phi_exists = header_block.instructions.iter().any(|inst| matches!(inst, Instruction::Phi { .. }));
        assert!(phi_exists, "Header block should have a Phi for x");
    }
}
