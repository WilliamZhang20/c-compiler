use model::{Type, Stmt as AstStmt, Block as AstBlock, Expr as AstExpr};
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
}
