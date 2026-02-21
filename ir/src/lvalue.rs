use model::{UnaryOp, Type, Expr as AstExpr};
use crate::types::{VarId, Operand, Instruction};
use crate::lowerer::Lowerer;

/// L-value (address) lowering implementation
impl Lowerer {
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
                        // For pointer indexing, we need the pointer's value, not its address.
                        // The base may be a Var or a Global (e.g. string literal "..."[i]).
                        let operand = self.lower_expr(array)?;
                        match operand {
                            Operand::Var(v) => v,
                            // String literals and globals: materialise into a tmp var first.
                            other => {
                                let tmp = self.new_var();
                                let bid = self.current_block.ok_or("Index outside of block")?;
                                self.blocks[bid.0].instructions.push(Instruction::Copy {
                                    dest: tmp,
                                    src: other,
                                });
                                tmp
                            }
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
                let bid = self.current_block.ok_or("Index outside of block")?;
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
