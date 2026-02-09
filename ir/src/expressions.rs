use model::{BinaryOp, UnaryOp, Type, Expr as AstExpr};
use crate::types::{VarId, Operand, Instruction};
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
                let l_ty = self.get_expr_type(left);
                let r_ty = self.get_expr_type(right);

                let mut l_val = self.lower_expr(left)?;
                let mut r_val = self.lower_expr(right)?;

                // Handle pointer arithmetic
                if *op == BinaryOp::Add || *op == BinaryOp::Sub {
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
                    self.add_instruction(Instruction::Load {
                        dest,
                        addr: Operand::Var(addr),
                        value_type: var_type,
                    });
                    Ok(Operand::Var(dest))
                }
            }
            AstExpr::Variable(_name) => {
                // Global variables or other variables not in allocas
                let addr = self.lower_to_addr(expr)?;
                let dest = self.new_var();
                let value_type = self.get_expr_type(expr);
                self.add_instruction(Instruction::Load {
                    dest,
                    addr: Operand::Var(addr),
                    value_type,
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Index { .. } | AstExpr::Member { .. } | AstExpr::PtrMember { .. } | AstExpr::Unary { op: UnaryOp::Deref, .. } => {
                let addr = self.lower_to_addr(expr)?;
                let dest = self.new_var();
                let value_type = self.get_expr_type(expr);
                self.add_instruction(Instruction::Load {
                    dest,
                    addr: Operand::Var(addr),
                    value_type,
                });
                Ok(Operand::Var(dest))
            }
            AstExpr::Unary { op, expr } => {
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
                let mut ir_args = Vec::new();
                for arg in args {
                    ir_args.push(self.lower_expr(arg)?);
                }
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

    /// Lower an expression to its address (for l-values)
    pub(crate) fn lower_to_addr(&mut self, expr: &AstExpr) -> Result<VarId, String> {
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
                let array_type = self.get_expr_type(array);
                let base_addr = match &array_type {
                    Type::Pointer(_) => {
                        // For pointer indexing, we need the pointer's value, not its address
                        match self.lower_expr(array)? {
                            Operand::Var(v) => v,
                            _ => return Err("Pointer indexing requires a variable".to_string()),
                        }
                    }
                    _ => {
                        // For array indexing, we need the array's address
                        self.lower_to_addr(array)?
                    }
                };
                let index_val = self.lower_expr(index)?;
                let dest = self.new_var();
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
                // Get the struct/union type from the expression
                let expr_type = self.get_expr_type(expr);
                let type_name = match &expr_type {
                    Type::Struct(name) => name.clone(),
                    Type::Union(name) => name.clone(),
                    _ => return Err(format!("Member access on non-struct/union type {:?}", expr_type)),
                };
                let (offset, _) = self.get_member_offset(&type_name, member); 
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
                // Get the struct/union type from the pointer
                let expr_type = self.get_expr_type(expr);
                let type_name = match &expr_type {
                    Type::Pointer(inner) => {
                        match &**inner {
                            Type::Struct(name) => name.clone(),
                            Type::Union(name) => name.clone(),
                            _ => return Err(format!("Pointer member access on non-struct/union pointer {:?}", expr_type)),
                        }
                    }
                    _ => return Err(format!("-> operator on non-pointer type {:?}", expr_type)),
                };
                let (offset, _) = self.get_member_offset(&type_name, member);
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
}
