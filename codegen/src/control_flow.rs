// Control flow code generation: terminators and phi resolution
// Extracted from function.rs: get_current_block_id, resolve_phis, gen_terminator

use crate::x86::{X86Reg, X86Operand, X86Instr};
use model::Type;
use ir::{Function as IrFunction, BlockId, Instruction as IrInstruction, Terminator as IrTerminator};
use crate::function::FunctionGenerator;

impl<'a> FunctionGenerator<'a> {
    pub(crate) fn get_current_block_id(&self) -> BlockId {
        self.current_block
    }

    pub(crate) fn resolve_phis(&mut self, target: BlockId, from: BlockId, func: &IrFunction) {
        let target_block = match func.blocks.iter().find(|b| b.id == target) {
            Some(b) => b,
            None => return,
        };
        for inst in &target_block.instructions {
            if let IrInstruction::Phi { dest, preds } = inst {
                for (pred_id, src_var) in preds {
                    if *pred_id == from {
                         let d_op = self.var_to_op(*dest);
                         // Handle alloca phi
                         if let Some(off) = self.alloca_buffers.get(src_var) {
                              self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *off)));
                              self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                         } else {
                              let s_op = self.var_to_op(*src_var);
                              if matches!(d_op, X86Operand::FloatMem(..)) {
                                  self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                                  self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                              } else if matches!(d_op, X86Operand::DoubleMem(..)) {
                                  self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
                                  self.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                              } else {
                                  // Check if we can emit a direct move (no intermediate register needed)
                                  let src_is_mem = matches!(s_op, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                                  let dst_is_mem = matches!(d_op, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                                  
                                  if !src_is_mem || !dst_is_mem {
                                      // At least one is a register or immediate — direct move is valid
                                      // But handle size mismatches
                                      let src_is_dword = matches!(s_op, X86Operand::DwordMem(..));
                                      let dst_is_dword = matches!(d_op, X86Operand::DwordMem(..));
                                      
                                      if src_is_dword && !dst_is_dword && matches!(d_op, X86Operand::Reg(_)) {
                                          // 32-bit mem → 64-bit reg: need movsx or two-step
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                                      } else if !src_is_dword && dst_is_dword && matches!(s_op, X86Operand::Reg(_)) {
                                          // 64-bit reg → 32-bit mem: need truncation
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                                      } else {
                                          // Direct move is safe (reg←reg, reg←imm, mem←imm, etc.)
                                          self.asm.push(X86Instr::Mov(d_op, s_op));
                                      }
                                  } else {
                                      // Both are memory — need intermediate register
                                      let src_is_dword = matches!(s_op, X86Operand::DwordMem(..));
                                      let dst_is_dword = matches!(d_op, X86Operand::DwordMem(..));
                                      
                                      if src_is_dword && dst_is_dword {
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                                      } else if src_is_dword {
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                                      } else if dst_is_dword {
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                                      } else {
                                          self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                                          self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                                      }
                                  }
                              }
                         }
                    }
                }
            }
        }
    }
    
    pub(crate) fn gen_terminator(&mut self, term: &IrTerminator, func_name: &str, func: &IrFunction) {
        match term {
            IrTerminator::Ret(op) => {
                if let Some(o) = op {
                    // Check for struct return type
                    let struct_class = crate::call_ops::classify_struct_arg(self, &func.return_type);
                    if let Some(class) = struct_class {
                        // Struct return: load eightbytes into RAX[:RDX]
                        let val = self.operand_to_op(o);
                        match class {
                            crate::call_ops::StructArgClass::OneReg => {
                                // Load first eightbyte from struct buffer
                                if let ir::Operand::Var(v) = o {
                                    if let Some(&off) = self.alloca_buffers.get(v) {
                                        self.asm.push(X86Instr::Mov(
                                            X86Operand::Reg(X86Reg::Rax),
                                            X86Operand::Mem(X86Reg::Rbp, off),
                                        ));
                                    } else {
                                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                                    }
                                } else {
                                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                                }
                            }
                            crate::call_ops::StructArgClass::TwoReg => {
                                if let ir::Operand::Var(v) = o {
                                    if let Some(&off) = self.alloca_buffers.get(v) {
                                        self.asm.push(X86Instr::Mov(
                                            X86Operand::Reg(X86Reg::Rax),
                                            X86Operand::Mem(X86Reg::Rbp, off),
                                        ));
                                        self.asm.push(X86Instr::Mov(
                                            X86Operand::Reg(X86Reg::Rdx),
                                            X86Operand::Mem(X86Reg::Rbp, off + 8),
                                        ));
                                    } else {
                                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                                    }
                                } else {
                                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                                }
                            }
                            crate::call_ops::StructArgClass::Memory => {
                                // Large struct already written via hidden pointer
                            }
                        }
                    } else {
                    let is_float_return = matches!(func.return_type, Type::Float | Type::Double);
                    let is_double_return = matches!(func.return_type, Type::Double);
                    if is_float_return {
                        let label = self.operand_to_op(o);
                        if is_double_return {
                            self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), label));
                        } else {
                            self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                        }
                    } else {
                        let val = self.operand_to_op(o);
                        // Handle 32-bit vs 64-bit return values
                        match val {
                            X86Operand::DwordMem(..) => {
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), val));
                            }
                            X86Operand::Imm(i) if i >= i32::MIN as i64 && i <= i32::MAX as i64 => {
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), val));
                            }
                            _ => {
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                            }
                        }
                    }
                    }
                }
                
                if !self.current_saved_regs.is_empty() {
                    let offset = (self.current_saved_regs.len() * 8) as i32;
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rsp), X86Operand::Mem(X86Reg::Rbp, -offset)));
                    for reg in self.current_saved_regs.iter().rev() {
                        self.asm.push(X86Instr::Pop(reg.clone()));
                    }
                    self.asm.push(X86Instr::Pop(X86Reg::Rbp));
                } else {
                    self.asm.push(X86Instr::Leave);
                }
                
                self.asm.push(X86Instr::Ret);
            }
            IrTerminator::Br(id) => {
                let current_bid = self.get_current_block_id();
                self.resolve_phis(*id, current_bid, func);
                self.asm.push(X86Instr::Jmp(format!("{}_{}", func_name, id.0)));
            }
            IrTerminator::CondBr { cond, then_block, else_block } => {
                let c_op = self.operand_to_op(cond);
                let current_bid = self.get_current_block_id();
                
                if let X86Operand::Reg(reg) = &c_op {
                    self.asm.push(X86Instr::Test(X86Operand::Reg(reg.clone()), X86Operand::Reg(reg.clone())));
                } else {
                    self.asm.push(X86Instr::Cmp(c_op, X86Operand::Imm(0)));
                }
                self.asm.push(X86Instr::Jcc("ne".to_string(), format!("temp_then_{}_{}", func_name, then_block.0)));
                
                self.resolve_phis(*else_block, current_bid, func);
                self.asm.push(X86Instr::Jmp(format!("{}_{}", func_name, else_block.0)));
                
                self.asm.push(X86Instr::Label(format!("temp_then_{}_{}", func_name, then_block.0)));
                self.resolve_phis(*then_block, current_bid, func);
                self.asm.push(X86Instr::Jmp(format!("{}_{}", func_name, then_block.0)));
            }
            _ => {
                // Trap/Unreachable -> Ret
                if !self.current_saved_regs.is_empty() {
                     let offset = (self.current_saved_regs.len() * 8) as i32;
                     self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rsp), X86Operand::Mem(X86Reg::Rbp, -offset)));
                     for reg in self.current_saved_regs.iter().rev() {
                         self.asm.push(X86Instr::Pop(reg.clone()));
                     }
                } else {
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rsp), X86Operand::Reg(X86Reg::Rbp)));
                }
                self.asm.push(X86Instr::Pop(X86Reg::Rbp));
                self.asm.push(X86Instr::Ret);
            }
        }
    }
}
