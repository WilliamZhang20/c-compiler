use model::{BinaryOp, UnaryOp, Type, Expr as AstExpr};
use crate::types::{Operand, Instruction, Terminator};
use crate::lowerer::Lowerer;

/// Expression lowering implementation
impl Lowerer {
    /// Lower an AST expression to an IR operand
    pub(crate) fn lower_expr(&mut self, expr: &AstExpr) -> Result<Operand, String> {
        match expr {
            AstExpr::Constant(c) => Ok(Operand::Constant(*c)),
            AstExpr::FloatConstant(f) => Ok(Operand::FloatConstant(*f)),
            AstExpr::Binary { left, op, right } => {
                if *op == BinaryOp::Assign {
                    let val = self.lower_expr(right)?;
                    let addr = self.lower_to_addr(left)?;
                    let value_type = self.get_expr_type(left);
                    self.add_instruction(Instruction::Store {
                        addr: Operand::Var(addr),
                        src: val.clone(),
                        value_type,
                    });
                    return Ok(val);
                }

                // Short-circuit logical AND: a && b
                // If a == 0, result = 0; else result = b (with short-circuit)
                if *op == BinaryOp::LogicalAnd {
                    let lhs_val = self.lower_expr(left)?;
                    let entry_bid = self.current_block.ok_or("LogicalAnd outside block")?;

                    let rhs_id   = self.new_block();
                    let false_id = self.new_block();
                    let merge_id = self.new_block();

                    self.blocks[entry_bid.0].terminator = Terminator::CondBr {
                        cond: lhs_val,
                        then_block: rhs_id,
                        else_block: false_id,
                    };

                    // false_id: lhs was 0, emit 0
                    self.sealed_blocks.insert(false_id);
                    self.current_block = Some(false_id);
                    let false_var = self.new_var();
                    self.blocks[false_id.0].instructions.push(Instruction::Copy {
                        dest: false_var,
                        src: Operand::Constant(0),
                    });
                    self.blocks[false_id.0].terminator = Terminator::Br(merge_id);

                    // rhs_id: lhs was truthy, evaluate rhs
                    self.sealed_blocks.insert(rhs_id);
                    self.current_block = Some(rhs_id);
                    let rhs_val = self.lower_expr(right)?;
                    let rhs_var = self.new_var();
                    let rhs_bid = self.current_block.ok_or("LogicalAnd rhs outside block")?;
                    self.blocks[rhs_bid.0].instructions.push(Instruction::Copy {
                        dest: rhs_var,
                        src: rhs_val,
                    });
                    self.blocks[rhs_bid.0].terminator = Terminator::Br(merge_id);

                    // merge_id: phi result
                    self.sealed_blocks.insert(merge_id);
                    self.current_block = Some(merge_id);
                    let result = self.new_var();
                    self.blocks[merge_id.0].instructions.push(Instruction::Phi {
                        dest: result,
                        preds: vec![(false_id, false_var), (rhs_bid, rhs_var)],
                    });
                    return Ok(Operand::Var(result));
                }

                // Short-circuit logical OR: a || b
                // If a != 0, result = 1; else result = b
                if *op == BinaryOp::LogicalOr {
                    let lhs_val = self.lower_expr(left)?;
                    let entry_bid = self.current_block.ok_or("LogicalOr outside block")?;

                    let rhs_id  = self.new_block();
                    let true_id = self.new_block();
                    let merge_id = self.new_block();

                    self.blocks[entry_bid.0].terminator = Terminator::CondBr {
                        cond: lhs_val,
                        then_block: true_id,
                        else_block: rhs_id,
                    };

                    // true_id: lhs was truthy, emit 1
                    self.sealed_blocks.insert(true_id);
                    self.current_block = Some(true_id);
                    let true_var = self.new_var();
                    self.blocks[true_id.0].instructions.push(Instruction::Copy {
                        dest: true_var,
                        src: Operand::Constant(1),
                    });
                    self.blocks[true_id.0].terminator = Terminator::Br(merge_id);

                    // rhs_id: lhs was 0, evaluate rhs
                    self.sealed_blocks.insert(rhs_id);
                    self.current_block = Some(rhs_id);
                    let rhs_val = self.lower_expr(right)?;
                    let rhs_var = self.new_var();
                    let rhs_bid = self.current_block.ok_or("LogicalOr rhs outside block")?;
                    self.blocks[rhs_bid.0].instructions.push(Instruction::Copy {
                        dest: rhs_var,
                        src: rhs_val,
                    });
                    self.blocks[rhs_bid.0].terminator = Terminator::Br(merge_id);

                    // merge_id: phi result
                    self.sealed_blocks.insert(merge_id);
                    self.current_block = Some(merge_id);
                    let result = self.new_var();
                    self.blocks[merge_id.0].instructions.push(Instruction::Phi {
                        dest: result,
                        preds: vec![(true_id, true_var), (rhs_bid, rhs_var)],
                    });
                    return Ok(Operand::Var(result));
                }

                // Handle compound assignments
                if matches!(op, BinaryOp::AddAssign | BinaryOp::SubAssign 
                    | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign 
                    | BinaryOp::BitwiseAndAssign | BinaryOp::BitwiseOrAssign 
                    | BinaryOp::BitwiseXorAssign | BinaryOp::ShiftLeftAssign 
                    | BinaryOp::ShiftRightAssign) 
                {
                    // 1. Get address of LHS
                    let addr = self.lower_to_addr(left)?;
                    
                    // 2. Load current value of LHS
                    let lhs_type = self.get_expr_type(left);
                    let curr_val_var = self.new_var();
                    self.var_types.insert(curr_val_var, lhs_type.clone());
                    self.add_instruction(Instruction::Load {
                        dest: curr_val_var,
                        addr: Operand::Var(addr),
                        value_type: lhs_type.clone(),
                    });
                    
                    // 3. Evaluate RHS
                    let rhs_val = self.lower_expr(right)?;
                    
                    // 4. Perform operation
                    let binary_op = match op {
                        BinaryOp::AddAssign => BinaryOp::Add,
                        BinaryOp::SubAssign => BinaryOp::Sub,
                        BinaryOp::MulAssign => BinaryOp::Mul,
                        BinaryOp::DivAssign => BinaryOp::Div,
                        BinaryOp::ModAssign => BinaryOp::Mod,
                        BinaryOp::BitwiseAndAssign => BinaryOp::BitwiseAnd,
                        BinaryOp::BitwiseOrAssign => BinaryOp::BitwiseOr,
                        BinaryOp::BitwiseXorAssign => BinaryOp::BitwiseXor,
                        BinaryOp::ShiftLeftAssign => BinaryOp::ShiftLeft,
                        BinaryOp::ShiftRightAssign => BinaryOp::ShiftRight,
                        _ => unreachable!(),
                    };
                    
                    // Handle pointer arithmetic for += and -=
                    let result_var = if (matches!(binary_op, BinaryOp::Add | BinaryOp::Sub)) 
                        && (matches!(lhs_type, Type::Pointer(_) | Type::Array(..))) 
                    {
                        // Pointer arithmetic: scale the RHS by element size
                        let inner_type = match &lhs_type {
                            Type::Pointer(inner) => inner,
                            Type::Array(inner, _) => inner,
                            _ => unreachable!(),
                        };
                        let size = self.get_type_size(inner_type);
                        
                        // Optimize: skip scaling if element size is 1
                        let scaled_rhs = if size > 1 {
                            let scaled_rhs_var = self.new_var();
                            self.add_instruction(Instruction::Binary {
                                dest: scaled_rhs_var,
                                op: BinaryOp::Mul,
                                left: rhs_val,
                                right: Operand::Constant(size),
                            });
                            Operand::Var(scaled_rhs_var)
                        } else {
                            rhs_val
                        };
                        
                        let res = self.new_var();
                        self.add_instruction(Instruction::Binary {
                            dest: res,
                            op: binary_op,
                            left: Operand::Var(curr_val_var),
                            right: scaled_rhs,
                        });
                        res
                    } else if self.is_float_type(&lhs_type) {
                        let res = self.new_var();
                        self.add_instruction(Instruction::FloatBinary {
                            dest: res,
                            op: binary_op,
                            left: Operand::Var(curr_val_var),
                            right: rhs_val,
                        });
                        res
                    } else {
                        let res = self.new_var();
                        self.add_instruction(Instruction::Binary {
                            dest: res,
                            op: binary_op,
                            left: Operand::Var(curr_val_var),
                            right: rhs_val,
                        });
                        res
                    };
                    
                    // 5. Store result back to LHS
                    self.add_instruction(Instruction::Store {
                        addr: Operand::Var(addr),
                        src: Operand::Var(result_var),
                        value_type: lhs_type,
                    });
                    
                    return Ok(Operand::Var(result_var));
                }

                let l_ty = self.get_expr_type(left);
                let r_ty = self.get_expr_type(right);

                let mut l_val = self.lower_expr(left)?;
                let mut r_val = self.lower_expr(right)?;

                // Handle pointer arithmetic
                if *op == BinaryOp::Add || *op == BinaryOp::Sub {
                    // Special case: pointer - pointer = number of elements
                    if *op == BinaryOp::Sub && matches!(l_ty, Type::Pointer(_)) && matches!(r_ty, Type::Pointer(_)) {
                        // ptr - ptr: compute byte difference, then divide by element size
                        let dest = self.new_var();
                        self.add_instruction(Instruction::Binary {
                            dest,
                            op: BinaryOp::Sub,
                            left: l_val,
                            right: r_val,
                        });
                        
                        // Divide by element size to get number of elements
                        if let Type::Pointer(ref inner) = l_ty {
                            let size = self.get_type_size(inner);
                            if size > 1 {
                                let result_dest = self.new_var();
                                self.add_instruction(Instruction::Binary {
                                    dest: result_dest,
                                    op: BinaryOp::Div,
                                    left: Operand::Var(dest),
                                    right: Operand::Constant(size),
                                });
                                return Ok(Operand::Var(result_dest));
                            }
                        }
                        return Ok(Operand::Var(dest));
                    }
                    
                    // Regular pointer arithmetic: ptr +/- int
                    if let Type::Pointer(ref inner) = l_ty {
                        let size = self.get_type_size(inner);
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
                    } else if let Type::Array(ref inner, _) = l_ty {
                        let size = self.get_type_size(inner);
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
                        // Handle right side being a pointer (ptr + int -> int + ptr)
                        if let Type::Pointer(ref inner) = r_ty {
                            let size = self.get_type_size(inner);
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
                // Check if this is a floating-point operation
                if self.is_float_type(&l_ty) || self.is_float_type(&r_ty) {
                    self.add_instruction(Instruction::FloatBinary {
                        dest,
                        op: op.clone(),
                        left: l_val,
                        right: r_val,
                    });
                } else {
                    self.add_instruction(Instruction::Binary {
                        dest,
                        op: op.clone(),
                        left: l_val,
                        right: r_val,
                    });
                }
                Ok(Operand::Var(dest))
            }
            AstExpr::Unary { op, expr: inner } if *op == UnaryOp::AddrOf => {
                let addr = self.lower_to_addr(inner)?;
                Ok(Operand::Var(addr))
            }
            AstExpr::Variable(name) if self.enum_constants.contains_key(name) => {
                // Enum constant: return the integer value
                let value = self.enum_constants[name];
                Ok(Operand::Constant(value))
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
            AstExpr::Variable(name) if self.variable_allocas.contains_key(name) => {
                // Check if it's an array - arrays decay to pointers (return address without load)
                let var_type = self.symbol_table.get(name).cloned().unwrap_or(Type::Int);
                if matches!(var_type, Type::Array(..)) {
                    // Array decay: return address of first element
                    let addr = self.lower_to_addr(expr)?;
                    Ok(Operand::Var(addr))
                } else {
                    // Regular variable: load its value
                    let addr = self.lower_to_addr(expr)?;
                    let dest = self.new_var();
                    self.var_types.insert(dest, var_type.clone());
                    self.add_instruction(Instruction::Load {
                        dest,
                        addr: Operand::Var(addr),
                        value_type: var_type,
                    });
                    Ok(Operand::Var(dest))
                }
            }
            AstExpr::Variable(name) => {
                // Global variables or other variables not in allocas
                if self.global_vars.contains(name) {
                     let value_type = self.get_expr_type(expr);
                     // Global arrays decay to a pointer — return the address directly.
                     if matches!(value_type, Type::Array(..)) {
                         let dest = self.new_var();
                         let elem_type = if let Type::Array(inner, _) = &value_type {
                             Type::Pointer(inner.clone())
                         } else { unreachable!() };
                         self.var_types.insert(dest, elem_type);
                         self.add_instruction(Instruction::Copy {
                             dest,
                             src: Operand::Global(name.clone()),
                         });
                         return Ok(Operand::Var(dest));
                     }
                     let dest = self.new_var();
                     self.var_types.insert(dest, value_type.clone());
                     self.add_instruction(Instruction::Load {
                        dest,
                        addr: Operand::Global(name.clone()),
                        value_type,
                    });
                     Ok(Operand::Var(dest))
                } else {
                    let addr = self.lower_to_addr(expr)?;
                    let dest = self.new_var();
                    let value_type = self.get_expr_type(expr);
                    self.var_types.insert(dest, value_type.clone());
                    self.add_instruction(Instruction::Load {
                        dest,
                        addr: Operand::Var(addr),
                        value_type,
                    });
                    Ok(Operand::Var(dest))
                }
            }
            AstExpr::Index { .. } | AstExpr::Member { .. } | AstExpr::PtrMember { .. } | AstExpr::Unary { op: UnaryOp::Deref, .. } => {
                let addr = self.lower_to_addr(expr)?;
                let dest = self.new_var();
                let value_type = self.get_expr_type(expr);
                self.var_types.insert(dest, value_type.clone());
                self.add_instruction(Instruction::Load {
                    dest,
                    addr: Operand::Var(addr),
                    value_type,
                });
                Ok(Operand::Var(dest))
            }            AstExpr::PostfixIncrement(expr) => {
                // For postfix: return old value, but modify the variable
                // 1. Compute type once
                let expr_type = self.get_expr_type(expr);
                let is_float = self.is_float_type(&expr_type);
                let increment = if matches!(&expr_type, Type::Pointer(_) | Type::Array(..)) {
                    let inner_type = match &expr_type {
                        Type::Pointer(inner) => inner,
                        Type::Array(inner, _) => inner,
                        _ => unreachable!(),
                    };
                    self.get_type_size(inner_type)
                } else {
                    1
                };
                
                // 2. Get the address
                let addr = self.lower_to_addr(expr)?;
                // 3. Load old value
                let old_val_var = self.new_var();
                self.var_types.insert(old_val_var, expr_type.clone());
                self.add_instruction(Instruction::Load {
                    dest: old_val_var,
                    addr: Operand::Var(addr),
                    value_type: expr_type.clone(),
                });
                // 4. Compute new value (old + 1)
                let new_val_var = self.new_var();
                if is_float {
                    self.add_instruction(Instruction::FloatBinary {
                        dest: new_val_var,
                        op: BinaryOp::Add,
                        left: Operand::Var(old_val_var),
                        right: Operand::FloatConstant(1.0),
                    });
                } else {
                    self.add_instruction(Instruction::Binary {
                        dest: new_val_var,
                        op: BinaryOp::Add,
                        left: Operand::Var(old_val_var),
                        right: Operand::Constant(increment),
                    });
                }
                // 5. Store new value back
                self.add_instruction(Instruction::Store {
                    addr: Operand::Var(addr),
                    src: Operand::Var(new_val_var),
                    value_type: expr_type,
                });
                // 6. Return old value
                Ok(Operand::Var(old_val_var))
            }
            AstExpr::PostfixDecrement(expr) => {
                // For postfix: return old value, but modify the variable
                // 1. Compute type once
                let expr_type = self.get_expr_type(expr);
                let is_float = self.is_float_type(&expr_type);
                let increment = if matches!(&expr_type, Type::Pointer(_) | Type::Array(..)) {
                    let inner_type = match &expr_type {
                        Type::Pointer(inner) => inner,
                        Type::Array(inner, _) => inner,
                        _ => unreachable!(),
                    };
                    self.get_type_size(inner_type)
                } else {
                    1
                };
                
                // 2. Get the address
                let addr = self.lower_to_addr(expr)?;
                // 3. Load old value
                let old_val_var = self.new_var();
                self.var_types.insert(old_val_var, expr_type.clone());
                self.add_instruction(Instruction::Load {
                    dest: old_val_var,
                    addr: Operand::Var(addr),
                    value_type: expr_type.clone(),
                });
                // 4. Compute new value (old - 1)
                let new_val_var = self.new_var();
                if is_float {
                    self.add_instruction(Instruction::FloatBinary {
                        dest: new_val_var,
                        op: BinaryOp::Sub,
                        left: Operand::Var(old_val_var),
                        right: Operand::FloatConstant(1.0),
                    });
                } else {
                    self.add_instruction(Instruction::Binary {
                        dest: new_val_var,
                        op: BinaryOp::Sub,
                        left: Operand::Var(old_val_var),
                        right: Operand::Constant(increment),
                    });
                }
                // 5. Store new value back
                self.add_instruction(Instruction::Store {
                    addr: Operand::Var(addr),
                    src: Operand::Var(new_val_var),
                    value_type: expr_type,
                });
                // 6. Return old value
                Ok(Operand::Var(old_val_var))
            }
            AstExpr::PrefixIncrement(expr) => {
                // For prefix: return new value after modification
                // 1. Compute type once
                let expr_type = self.get_expr_type(expr);
                let is_float = self.is_float_type(&expr_type);
                let increment = if matches!(&expr_type, Type::Pointer(_) | Type::Array(..)) {
                    let inner_type = match &expr_type {
                        Type::Pointer(inner) => inner,
                        Type::Array(inner, _) => inner,
                        _ => unreachable!(),
                    };
                    self.get_type_size(inner_type)
                } else {
                    1
                };
                
                // 2. Get the address
                let addr = self.lower_to_addr(expr)?;
                // 3. Load old value
                let old_val_var = self.new_var();
                self.var_types.insert(old_val_var, expr_type.clone());
                self.add_instruction(Instruction::Load {
                    dest: old_val_var,
                    addr: Operand::Var(addr),
                    value_type: expr_type.clone(),
                });
                // 4. Compute new value (old + 1)
                let new_val_var = self.new_var();
                if is_float {
                    self.add_instruction(Instruction::FloatBinary {
                        dest: new_val_var,
                        op: BinaryOp::Add,
                        left: Operand::Var(old_val_var),
                        right: Operand::FloatConstant(1.0),
                    });
                } else {
                    self.add_instruction(Instruction::Binary {
                        dest: new_val_var,
                        op: BinaryOp::Add,
                        left: Operand::Var(old_val_var),
                        right: Operand::Constant(increment),
                    });
                }
                // 5. Store new value back
                self.add_instruction(Instruction::Store {
                    addr: Operand::Var(addr),
                    src: Operand::Var(new_val_var),
                    value_type: expr_type,
                });
                // 6. Return new value
                Ok(Operand::Var(new_val_var))
            }
            AstExpr::PrefixDecrement(expr) => {
                // For prefix: return new value after modification
                // 1. Compute type once
                let expr_type = self.get_expr_type(expr);
                let is_float = self.is_float_type(&expr_type);
                let increment = if matches!(&expr_type, Type::Pointer(_) | Type::Array(..)) {
                    let inner_type = match &expr_type {
                        Type::Pointer(inner) => inner,
                        Type::Array(inner, _) => inner,
                        _ => unreachable!(),
                    };
                    self.get_type_size(inner_type)
                } else {
                    1
                };
                
                // 2. Get the address
                let addr = self.lower_to_addr(expr)?;
                // 3. Load old value
                let old_val_var = self.new_var();
                self.var_types.insert(old_val_var, expr_type.clone());
                self.add_instruction(Instruction::Load {
                    dest: old_val_var,
                    addr: Operand::Var(addr),
                    value_type: expr_type.clone(),
                });
                // 4. Compute new value (old - 1)
                let new_val_var = self.new_var();
                if is_float {
                    self.add_instruction(Instruction::FloatBinary {
                        dest: new_val_var,
                        op: BinaryOp::Sub,
                        left: Operand::Var(old_val_var),
                        right: Operand::FloatConstant(1.0),
                    });
                } else {
                    self.add_instruction(Instruction::Binary {
                        dest: new_val_var,
                        op: BinaryOp::Sub,
                        left: Operand::Var(old_val_var),
                        right: Operand::Constant(increment),
                    });
                }
                // 5. Store new value back
                self.add_instruction(Instruction::Store {
                    addr: Operand::Var(addr),
                    src: Operand::Var(new_val_var),
                    value_type: expr_type,
                });
                // 6. Return new value
                Ok(Operand::Var(new_val_var))
            }            AstExpr::Unary { op, expr } => {
                let val = self.lower_expr(expr)?;
                let dest = self.new_var();
                let expr_ty = self.get_expr_type(expr);
                if self.is_float_type(&expr_ty) {
                    self.add_instruction(Instruction::FloatUnary {
                        dest,
                        op: op.clone(),
                        src: val,
                    });
                } else {
                    self.add_instruction(Instruction::Unary {
                        dest,
                        op: op.clone(),
                        src: val,
                    });
                }
                Ok(Operand::Var(dest))
            }
            AstExpr::StringLiteral(s) => {
                let label = format!("str_{}", self.global_strings.len());
                self.global_strings.push((label.clone(), s.clone()));
                Ok(Operand::Global(label))
            }
            AstExpr::Call { func, args } => {
                // Handle intrinsics that require l-value arguments (pass-by-reference semantics)
                if let AstExpr::Variable(name) = func.as_ref() {
                    if name == "__builtin_va_start" {
                        if args.len() >= 2 {
                            let list_addr = self.lower_to_addr(&args[0])?;
                            
                            // Find index of second argument (last named parameter)
                            let arg_index = if let AstExpr::Variable(name) = &args[1] {
                                *self.param_indices.get(name).ok_or(format!("__builtin_va_start argument '{}' must be a parameter name", name))?
                            } else {
                                return Err("__builtin_va_start second argument must be a variable name".to_string());
                            };
                            
                            let bid = self.current_block.ok_or("VaStart outside block")?;
                            self.blocks[bid.0].instructions.push(Instruction::VaStart {
                                list: Operand::Var(list_addr),
                                arg_index,
                            });
                            return Ok(Operand::Constant(0));
                        }
                    } else if name == "__builtin_va_end" {
                        if !args.is_empty() {
                            let list_addr = self.lower_to_addr(&args[0])?;
                            let bid = self.current_block.ok_or("VaEnd outside block")?;
                            self.blocks[bid.0].instructions.push(Instruction::VaEnd {
                                list: Operand::Var(list_addr),
                            });
                            return Ok(Operand::Constant(0));
                        }
                    } else if name == "__builtin_va_copy" {
                        if args.len() >= 2 {
                            let dest_addr = self.lower_to_addr(&args[0])?;
                            let src_val = self.lower_expr(&args[1])?;
                            let bid = self.current_block.ok_or("VaCopy outside block")?;
                            self.blocks[bid.0].instructions.push(Instruction::VaCopy {
                                dest: Operand::Var(dest_addr),
                                src: src_val,
                            });
                            return Ok(Operand::Constant(0));
                        }
                    }
                }

                let mut ir_args = Vec::new();
                for arg in args {
                    ir_args.push(self.lower_expr(arg)?);
                }
                
                // Re-read current_block AFTER lowering args, since ternary expressions
                // in arguments can create new basic blocks and change current_block
                let bid = self.current_block.ok_or("Call outside block")?;
                let dest = self.new_var();
                
                // Check if it's a direct call (function name) or indirect call (function pointer variable)
                // If it's a Variable that's not a local, assume it's a function (could be external/forward-declared)
                let is_direct_call = if let AstExpr::Variable(name) = func.as_ref() {
                    !self.is_local(name)  // Not a local variable means it's a function
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
                    let bid = self.current_block.ok_or("IndirectCall outside block")?;
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
            AstExpr::SizeOfExpr(expr) => {
                let expr_type = self.get_expr_type(expr);
                Ok(Operand::Constant(self.get_type_size(&expr_type)))
            }
            AstExpr::Cast(ty, expr) => {
                let src_val = self.lower_expr(expr)?;
                // Check if this is a type conversion (not just a pointer cast)
                let src_type = self.get_operand_type(&src_val)?;
                
                // If types are the same, no conversion needed
                if &src_type == ty {
                    return Ok(src_val);
                }
                
                // Check if this requires a float<->int conversion
                let src_is_float = matches!(src_type, Type::Float | Type::Double);
                let dest_is_float = matches!(ty, Type::Float | Type::Double);
                
                if src_is_float != dest_is_float {
                    // This is a float<->int conversion, generate a Copy instruction
                    // The codegen layer will handle this specially  
                    let dest = self.new_var();
                    // Record the destination type
                    self.var_types.insert(dest, ty.clone());
                    
                    let bid = self.current_block.ok_or("Cast outside block")?;
                    self.blocks[bid.0].instructions.push(Instruction::Cast {
                        dest,
                        src: src_val,
                        r#type: ty.clone(),
                    });
                    return Ok(Operand::Var(dest));
                }
                
                // For other casts (like int to int of different sizes, or pointer casts),
                // just return the source value (no conversion needed in SSA form)
                Ok(src_val)
            }
            AstExpr::Conditional { condition, then_expr, else_expr } => {
                // Evaluate condition in the current block.
                let cond_val = self.lower_expr(condition)?;
                let entry_bid = self.current_block.ok_or("Ternary outside block")?;

                let then_id  = self.new_block();
                let else_id  = self.new_block();
                let merge_id = self.new_block();

                self.blocks[entry_bid.0].terminator = Terminator::CondBr {
                    cond: cond_val,
                    then_block: then_id,
                    else_block: else_id,
                };

                // Then branch – evaluate then_expr and materialise it into a var.
                self.sealed_blocks.insert(then_id);
                self.current_block = Some(then_id);
                let then_operand = self.lower_expr(then_expr)?;
                let then_var = self.new_var();
                let then_bid = self.current_block.ok_or("Ternary then outside block")?;
                self.blocks[then_bid.0].instructions.push(Instruction::Copy {
                    dest: then_var,
                    src: then_operand,
                });
                self.blocks[then_bid.0].terminator = Terminator::Br(merge_id);

                // Else branch – evaluate else_expr and materialise it into a var.
                self.sealed_blocks.insert(else_id);
                self.current_block = Some(else_id);
                let else_operand = self.lower_expr(else_expr)?;
                let else_var = self.new_var();
                let else_bid = self.current_block.ok_or("Ternary else outside block")?;
                self.blocks[else_bid.0].instructions.push(Instruction::Copy {
                    dest: else_var,
                    src: else_operand,
                });
                self.blocks[else_bid.0].terminator = Terminator::Br(merge_id);

                // Merge block – Phi to select the result.
                self.sealed_blocks.insert(merge_id);
                self.current_block = Some(merge_id);
                let result = self.new_var();
                let merge_bid = merge_id; // already known
                self.blocks[merge_bid.0].instructions.push(Instruction::Phi {
                    dest: result,
                    preds: vec![(then_bid, then_var), (else_bid, else_var)],
                });
                Ok(Operand::Var(result))
            }
            AstExpr::CompoundLiteral { r#type, init } => {
                // Compound literal: allocate anonymous local, initialize it,
                // and return either a pointer (for aggregates) or the value.
                let bid = self.current_block.ok_or("CompoundLiteral outside block")?;
                let alloca = self.new_var();
                let ty = r#type.clone();
                self.blocks[bid.0].instructions.push(Instruction::Alloca {
                    dest: alloca,
                    r#type: ty.clone(),
                });

                // Dispatch to the correct init-list helper based on type.
                match &ty {
                    Type::Array(inner, _) => {
                        let elem_size = self.get_type_size(inner);
                        self.lower_init_list_to_stores(alloca, init, inner, elem_size, bid)?;
                    }
                    Type::Struct(_) | Type::Union(_) => {
                        self.lower_struct_init_list(alloca, &ty, init, bid)?;
                    }
                    _ => {
                        // Scalar compound literal, e.g. (int){42}
                        if let Some(item) = init.first() {
                            let val = self.lower_expr(&item.value)?;
                            self.blocks[bid.0].instructions.push(Instruction::Store {
                                addr: Operand::Var(alloca),
                                src: val,
                                value_type: ty.clone(),
                            });
                        }
                    }
                }

                // For aggregates, the compound literal evaluates to the
                // address of the temporary (like an array name).  For scalars,
                // load the value back out.
                match &ty {
                    Type::Array(..) | Type::Struct(_) | Type::Union(_) => {
                        Ok(Operand::Var(alloca))
                    }
                    _ => {
                        let result = self.new_var();
                        self.blocks[bid.0].instructions.push(Instruction::Load {
                            dest: result,
                            addr: Operand::Var(alloca),
                            value_type: ty,
                        });
                        Ok(Operand::Var(result))
                    }
                }
            }
            AstExpr::Comma(exprs) => {
                // Comma operator: evaluate each sub-expression left to right,
                // discarding all results except the last one.
                if exprs.is_empty() {
                    return Err("Empty comma expression".to_string());
                }
                let mut result = Operand::Constant(0);
                for e in exprs {
                    result = self.lower_expr(e)?;
                }
                Ok(result)
            }
            AstExpr::StmtExpr(stmts) => {
                // GNU statement expression: lower all statements, and the
                // value is the last expression-statement's value.
                if stmts.is_empty() {
                    return Ok(Operand::Constant(0));
                }
                // Lower all statements except the last one
                for stmt in &stmts[..stmts.len() - 1] {
                    self.lower_stmt(stmt)?;
                }
                // The last statement must be an expression statement
                match stmts.last().unwrap() {
                    model::Stmt::Expr(expr) => self.lower_expr(expr),
                    other => {
                        // Not an expression statement — lower it and return 0
                        self.lower_stmt(other)?;
                        Ok(Operand::Constant(0))
                    }
                }
            }
            AstExpr::InitList(_) => {
                // InitList is handled specially during declaration lowering,
                // not as a standalone expression.
                Err("InitList expression cannot be lowered standalone; it must appear as a declaration initializer".to_string())
            }
            AstExpr::BuiltinOffsetof { r#type, member } => {
                // __builtin_offsetof(type, member) → constant offset
                let struct_name = match r#type {
                    Type::Struct(name) => name.clone(),
                    Type::Union(name) => name.clone(),
                    _ => return Err(format!("__builtin_offsetof requires struct/union type, got {:?}", r#type)),
                };
                let (offset, _field_type) = self.get_member_offset(&struct_name, member);
                Ok(Operand::Constant(offset))
            }
        }
    }
}
