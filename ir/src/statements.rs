use model::{Type, Stmt as AstStmt, Block as AstBlock, Expr as AstExpr, BinaryOp};
use crate::types::{Operand, Instruction, Terminator};
use crate::lowerer::Lowerer;

/// Statement lowering implementation
impl Lowerer {
    /// Lower an AST block to IR
    pub(crate) fn lower_block(&mut self, block: &AstBlock) -> Result<(), String> {
        for stmt in &block.statements {
            self.lower_stmt(stmt)?;
        }
        Ok(())
    }

    /// Lower an AST statement to IR
    pub(crate) fn lower_stmt(&mut self, stmt: &AstStmt) -> Result<(), String> {
        // If we don't have a current block, create an unreachable one for dead code
        // This happens after goto, return, break, continue, etc.
        if self.current_block.is_none() && !matches!(stmt, AstStmt::Label(_) | AstStmt::Case(_) | AstStmt::Default) {
            // Create a dead code block
            let dead_block = self.new_block();
            self.seal_block(dead_block);
            self.current_block = Some(dead_block);
        }
        
        match stmt {
            AstStmt::Return(expr) => {
                let val = if let Some(e) = expr {
                    let mut v = self.lower_expr(e)?;
                    
                    // Handle implicit cast for return value
                    // Clone return type to avoid borrowing self while mutating self
                    if let Some(ret_type) = self.current_return_type.clone() {
                        let expr_type = self.get_operand_type(&v)?;
                        
                        let src_is_float = matches!(expr_type, Type::Float | Type::Double);
                        let dest_is_float = matches!(ret_type, Type::Float | Type::Double);
                        
                        if src_is_float != dest_is_float {
                             let dest = self.new_var();
                             self.var_types.insert(dest, ret_type.clone());
                             let bid = self.current_block.ok_or("Return cast outside block")?;
                             
                             self.blocks[bid.0].instructions.push(Instruction::Cast {
                                 dest,
                                 src: v,
                                 r#type: ret_type.clone(),
                             });
                             v = Operand::Var(dest);
                        }
                    }
                    Some(v)
                } else {
                    None
                };
                let bid = self.current_block.ok_or("Return outside of block")?;
                self.blocks[bid.0].terminator = Terminator::Ret(val);
                self.current_block = None; // Dead code after return
            }
            AstStmt::Declaration { r#type, qualifiers: _, name, init } => {
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
                    
                    // Handle array initialization (e.g., char arr[] = "string")
                    if let Some(init_expr) = init {
                        match init_expr {
                            AstExpr::StringLiteral(s) => {
                                // Initialize each character in the array
                                for (i, ch) in s.chars().chain(std::iter::once('\0')).enumerate() {
                                    // Calculate offset: base + i * element_size
                                    let elem_type = if let Type::Array(inner, _) = r#type {
                                        inner.as_ref().clone()
                                    } else {
                                        unreachable!()
                                    };
                                    
                                    let offset_var = if i == 0 {
                                        var
                                    } else {
                                        let idx_var = self.new_var();
                                        self.blocks[bid.0].instructions.push(Instruction::Binary {
                                            dest: idx_var,
                                            op: BinaryOp::Add,
                                            left: Operand::Var(var),
                                            right: Operand::Constant(i as i64),
                                        });
                                        idx_var
                                    };
                                    
                                    self.blocks[bid.0].instructions.push(Instruction::Store {
                                        addr: Operand::Var(offset_var),
                                        src: Operand::Constant(ch as i64),
                                        value_type: elem_type.clone(),
                                    });
                                }
                            }
                            _ => {
                                // TODO: Handle other array initializers (e.g., {1, 2, 3})
                            }
                        }
                    }
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
                            value_type: r#type.clone(),
                        });
                        
                        let var = match val {
                            Operand::Var(v) => v,
                            Operand::Constant(_) | Operand::FloatConstant(_) | Operand::Global(_) => {
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
            AstStmt::Label(name) => {
                // Create a new block for the label
                let label_block = self.new_block();
                
                // Mark this block as a label target (should not be merged by CFG optimizations)
                self.blocks[label_block.0].is_label_target = true;
                
                // Jump from current block to label block (if current block exists)
                if let Some(bid) = self.current_block {
                    // Only add branch if the current block doesn't already have a terminator
                    if matches!(self.blocks[bid.0].terminator, Terminator::Unreachable) {
                        self.blocks[bid.0].terminator = Terminator::Br(label_block);
                    }
                }
                
                // Register the label
                self.labels.insert(name.clone(), label_block);
                self.seal_block(label_block);
                self.current_block = Some(label_block);
                
                // Resolve any pending gotos to this label
                let mut i = 0;
                while i < self.pending_gotos.len() {
                    if self.pending_gotos[i].0 == *name {
                        let goto_block = self.pending_gotos[i].1;
                        self.blocks[goto_block.0].terminator = Terminator::Br(label_block);
                        self.pending_gotos.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }
            AstStmt::Goto(label) => {
                let bid = self.current_block.ok_or("Goto outside of block")?;
                
                // Check if label already exists (backward goto)
                if let Some(&label_block) = self.labels.get(label) {
                    self.blocks[bid.0].terminator = Terminator::Br(label_block);
                } else {
                    // Forward goto - store for later resolution
                    self.pending_gotos.push((label.clone(), bid));
                    // Temporary terminator, will be fixed when label is found
                    self.blocks[bid.0].terminator = Terminator::Unreachable;
                }
                self.current_block = None;  // Dead code after goto
            }
            AstStmt::InlineAsm { template, outputs, inputs, clobbers, is_volatile } => {
                // Lower inline assembly to IR
                let bid = self.current_block.ok_or("Inline assembly outside of block")?;
                
                // Map output expressions to VarIds
                let mut output_vars = Vec::new();
                for output in outputs {
                    // Output should be an lvalue (variable)
                    if let AstExpr::Variable(name) = &output.expr {
                        if let Some(&alloca_var) = self.variable_allocas.get(name) {
                            output_vars.push(alloca_var);
                        } else {
                            return Err(format!("Output variable {} not found for inline asm", name));
                        }
                    } else {
                        return Err("Inline assembly output must be a variable".to_string());
                    }
                }
                
                // Lower input expressions to operands
                let mut input_ops = Vec::new();
                for input in inputs {
                    input_ops.push(self.lower_expr(&input.expr)?);
                }
                
                self.blocks[bid.0].instructions.push(Instruction::InlineAsm {
                    template: template.clone(),
                    outputs: output_vars,
                    inputs: input_ops,
                    clobbers: clobbers.clone(),
                    is_volatile: *is_volatile,
                });
            }
        }
        Ok(())
    }
}
