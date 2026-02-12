use std::collections::HashMap;
use crate::x86::{X86Reg, X86Operand, X86Instr};
use model::Type;
use ir::{Function as IrFunction, BlockId, VarId, Operand, Instruction as IrInstruction, Terminator as IrTerminator};
use crate::regalloc::{PhysicalReg, allocate_registers};
use crate::instructions::InstructionGenerator;
use crate::types::TypeCalculator;
use crate::float_ops::{gen_float_binary_op, gen_float_unary_op};

/// Handles generation of code for a single function
pub struct FunctionGenerator<'a> {
    pub asm: Vec<X86Instr>,
    
    // Context from parent Codegen
    pub(crate) structs: &'a HashMap<String, model::StructDef>,
    pub(crate) unions: &'a HashMap<String, model::UnionDef>,
    pub(crate) func_return_types: &'a HashMap<String, Type>,
    pub(crate) float_constants: &'a mut HashMap<String, f64>,
    pub(crate) next_float_const: &'a mut usize,
    
    // Per-function state
    pub(crate) stack_slots: HashMap<VarId, i32>,
    pub(crate) next_slot: i32,
    pub(crate) reg_alloc: HashMap<VarId, PhysicalReg>,
    pub(crate) var_types: HashMap<VarId, Type>,
    pub(crate) alloca_buffers: HashMap<VarId, i32>,
    pub(crate) current_saved_regs: Vec<X86Reg>,
    pub(crate) enable_regalloc: bool,
}

impl<'a> FunctionGenerator<'a> {
    pub fn new(
        structs: &'a HashMap<String, model::StructDef>,
        unions: &'a HashMap<String, model::UnionDef>,
        func_return_types: &'a HashMap<String, Type>,
        float_constants: &'a mut HashMap<String, f64>,
        next_float_const: &'a mut usize,
        enable_regalloc: bool,
    ) -> Self {
        Self {
            asm: Vec::new(),
            structs,
            unions,
            func_return_types,
            float_constants,
            next_float_const,
            stack_slots: HashMap::new(),
            next_slot: 0,
            reg_alloc: HashMap::new(),
            var_types: HashMap::new(),
            alloca_buffers: HashMap::new(),
            current_saved_regs: Vec::new(),
            enable_regalloc,
        }
    }

    pub fn gen_function(mut self, func: &IrFunction) -> Vec<X86Instr> {
        // Perform register allocation
        if self.enable_regalloc {
            self.reg_alloc = allocate_registers(func);
        }
        
        // Identify used callee-saved registers
        self.current_saved_regs.clear();
        let used_regs: std::collections::HashSet<_> = self.reg_alloc.values().collect();
        for reg in PhysicalReg::callee_saved() {
            if used_regs.contains(&reg) {
                self.current_saved_regs.push(reg.to_x86());
            }
        }
        
        self.asm.push(X86Instr::Label(func.name.clone()));
        
        // Prologue
        self.asm.push(X86Instr::Push(X86Reg::Rbp));
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rbp), X86Operand::Reg(X86Reg::Rsp)));
        
        // Push callee-saved registers
        for reg in &self.current_saved_regs {
            self.asm.push(X86Instr::Push(reg.clone()));
        }
        
        // Account for pushed registers in stack slot allocation
        self.next_slot = (self.current_saved_regs.len() * 8) as i32;
        
        self.allocate_stack_slots(func);
        
        let saved_size = (self.current_saved_regs.len() * 8) as i32;
        let locals_size = self.next_slot - saved_size;
        let shadow_space = 32;
        
        // Align total stack frame to 16 bytes
        let total_stack = saved_size + locals_size + shadow_space;
        let aligned_total = (total_stack + 15) & !15;
        let sub_amount = aligned_total - saved_size;
        
        if sub_amount > 0 {
            self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rsp), X86Operand::Imm(sub_amount as i64)));
        }

        // Handle parameters (Windows ABI: RCX, RDX, R8, R9 for ints; XMM0-XMM3 for floats)
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
        let float_regs = [X86Reg::Xmm0, X86Reg::Xmm1, X86Reg::Xmm2, X86Reg::Xmm3];
        for (i, (param_type, var)) in func.params.iter().enumerate() {
            // Record parameter type for later use
            self.var_types.insert(*var, param_type.clone());
            
            let dest = self.var_to_op(*var);
            let is_float = matches!(param_type, Type::Float | Type::Double);
            
            if i < 4 {
                if is_float {
                    // Float parameters come in XMM registers
                    self.asm.push(X86Instr::Movss(dest, X86Operand::Reg(float_regs[i].clone())));
                } else {
                    // Integer/pointer parameters come in general-purpose registers
                    self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(param_regs[i].clone())));
                }
            } else {
                // Parameters beyond the 4th are on the stack
                let offset = 16 + 32 + (i - 4) * 8;
                if is_float {
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Movss(dest, X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(X86Reg::Rax)));
                }
            }
        }

        for block in &func.blocks {
            self.asm.push(X86Instr::Label(format!("{}_{}", func.name, block.id.0)));
            for inst in &block.instructions {
                self.gen_instr(inst);
            }
            self.gen_terminator(&block.terminator, &func.name, func);
        }

        self.asm
    }

    fn allocate_stack_slots(&mut self, func: &IrFunction) {
        // Only allocate stack slots for variables that:
        // 1. Need Alloca (arrays/structs) - these always need stack space
        // 2. Didn't get a register assigned (spilled variables)
        
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    IrInstruction::Alloca { dest, r#type } => {
                        // Alloca uses direct stack space, not managed by stack_slots map in the same way
                        // Instead, we just reserve a block of stack and track its offset
                        let size = self.get_type_size(r#type);
                        // Align to 16 bytes for SSE compatibility
                        let size = (size + 15) & !15;
                        
                        self.next_slot += size as i32;
                        let offset = -self.next_slot;
                        self.alloca_buffers.insert(*dest, offset);
                    }
                    IrInstruction::Binary { dest, .. } |
                    IrInstruction::FloatBinary { dest, .. } |
                    IrInstruction::Unary { dest, .. } |
                    IrInstruction::FloatUnary { dest, .. } |
                    IrInstruction::Phi { dest, .. } |
                    IrInstruction::Copy { dest, .. } |
                    IrInstruction::Cast { dest, .. } |
                    IrInstruction::Load { dest, .. } |
                    IrInstruction::GetElementPtr { dest, .. } => {
                        if !self.reg_alloc.contains_key(dest) {
                            self.get_or_create_slot(*dest);
                        }
                    }
                    IrInstruction::Call { dest, .. } => {
                       if let Some(d) = dest {
                           if !self.reg_alloc.contains_key(d) {
                               self.get_or_create_slot(*d);
                           }
                       } 
                    }
                    IrInstruction::IndirectCall { dest, .. } => {
                       if let Some(d) = dest {
                           if !self.reg_alloc.contains_key(d) {
                               self.get_or_create_slot(*d);
                           }
                       } 
                    }
                    IrInstruction::InlineAsm { outputs, .. } => {
                        for output in outputs {
                            if !self.reg_alloc.contains_key(output) {
                                self.get_or_create_slot(*output);
                            }
                        }
                    }
                    IrInstruction::Store { .. } => {}
                }
            }
        }
        
        // Parameters: only allocate stack if no register
        for (_, var) in &func.params {
            if !self.reg_alloc.contains_key(var) {
                self.get_or_create_slot(*var);
            }
        }
    }

    fn gen_instr(&mut self, inst: &IrInstruction) {
        match inst {
            IrInstruction::Cast { dest, src, r#type } => {
                self.var_types.insert(*dest, r#type.clone());
                let d_op = self.var_to_op(*dest);
                
                // Handle Alloca src (pointer cast)
                if let Operand::Var(var) = src {
                    if let Some(off) = self.alloca_buffers.get(var) {
                        let mem_op = X86Operand::Mem(X86Reg::Rbp, *off);
                         match &d_op {
                            X86Operand::Reg(_) => {
                                self.asm.push(X86Instr::Lea(d_op.clone(), mem_op));
                            }
                            _ => {
                                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), mem_op));
                                self.asm.push(X86Instr::Mov(d_op.clone(), X86Operand::Reg(X86Reg::Rax)));
                            }
                        }
                        return;
                    }
                }

                let dest_is_float = matches!(r#type, Type::Float | Type::Double);
                let src_is_float = match src {
                    Operand::FloatConstant(_) => true,
                    Operand::Var(v) => {
                         self.var_types.get(v).map(|t| matches!(t, Type::Float | Type::Double)).unwrap_or(false)
                    }
                    _ => false
                };
                
                let s_op = self.operand_to_op(src);

                if dest_is_float && !src_is_float {
                    // Int -> Float
                    let src_reg = if let X86Operand::Imm(_) = s_op {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op.clone()));
                        X86Operand::Reg(X86Reg::Eax)
                    } else if matches!(s_op, X86Operand::DwordMem(..) | X86Operand::Mem(..)) {
                         s_op.clone()
                    } else {
                        s_op.clone()
                    };
                    
                    self.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), src_reg));
                    self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                    
                } else if !dest_is_float && src_is_float {
                    // Float -> Int
                    let use_rax = !matches!(d_op, X86Operand::DwordMem(..));
                    let dst_reg = if use_rax { X86Reg::Rax } else { X86Reg::Eax };
                    
                    if matches!(s_op, X86Operand::FloatMem(..) | X86Operand::DwordMem(..)) {
                        self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                        self.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(dst_reg.clone()), X86Operand::Reg(X86Reg::Xmm0)));
                    } else {
                        self.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(dst_reg.clone()), s_op));
                    }
                    self.asm.push(X86Instr::Mov(d_op.clone(), X86Operand::Reg(dst_reg)));
                } else {
                    // Same type cast (or pointer cast etc) - just copy
                     if dest_is_float {
                         self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                         self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                     } else {
                         if matches!(d_op, X86Operand::DwordMem(..)) {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                         } else {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                         }
                     }
                }
            }
            IrInstruction::Copy { dest, src} => {
                 if !self.var_types.contains_key(dest) {
                    let inferred_src_type = if let Operand::Var(v) = src {
                        self.var_types.get(v).cloned()
                    } else if let Operand::FloatConstant(_) = src {
                        Some(Type::Float)
                    } else if let Operand::Constant(_) = src {
                        Some(Type::Int)
                    } else {
                        None
                    };
                    if let Some(t) = inferred_src_type {
                        self.var_types.insert(*dest, t);
                    }
                }
                
                let s_op = self.operand_to_op(src);
                let d_op = self.var_to_op(*dest);
                
                // Handle Global variables (load address)
                if let X86Operand::Label(name) = &s_op {
                    let rip_rel = X86Operand::RipRelLabel(name.clone());
                    match &d_op {
                        X86Operand::Reg(_) => {
                            self.asm.push(X86Instr::Lea(d_op.clone(), rip_rel));
                        }
                        _ => {
                            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), rip_rel));
                            self.asm.push(X86Instr::Mov(d_op.clone(), X86Operand::Reg(X86Reg::Rax)));
                        }
                    }
                    return;
                }
                
                // Handle Alloca addresses (arrays/structs on stack) -> LEA
                if let Operand::Var(var) = src {
                    if let Some(off) = self.alloca_buffers.get(var) {
                        let mem_op = X86Operand::Mem(X86Reg::Rbp, *off);
                        match &d_op {
                             X86Operand::Reg(_) => {
                                 self.asm.push(X86Instr::Lea(d_op.clone(), mem_op));
                             }
                             _ => {
                                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), mem_op));
                                 self.asm.push(X86Instr::Mov(d_op.clone(), X86Operand::Reg(X86Reg::Rax)));
                             }
                        }
                        return;
                    }
                }

                // If s_op is a Mem operand that matches an alloca buffer offset
                if let X86Operand::Mem(X86Reg::Rbp, offset) = s_op {
                    let is_alloca_buffer = self.alloca_buffers.values().any(|&buf_offset| buf_offset == offset);
                    if is_alloca_buffer {
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                        return;
                    }
                }
                
                // Check if float
                if matches!(s_op, X86Operand::FloatMem(..)) || matches!(d_op, X86Operand::FloatMem(..)) {
                     self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                     self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                } else if matches!(s_op, X86Operand::Mem(..)) && matches!(d_op, X86Operand::Mem(..)) {
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                     self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                } else {
                     self.asm.push(X86Instr::Mov(d_op, s_op));
                }
            }
            IrInstruction::Binary { dest, op, left, right } => {
                let l_op = if let Operand::Var(var) = left {
                    if let Some(off) = self.alloca_buffers.get(var) {
                         self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::R10), X86Operand::Mem(X86Reg::Rbp, *off)));
                         X86Operand::Reg(X86Reg::R10)
                    } else { self.operand_to_op(left) }
                } else if let Operand::Global(name) = left {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::R10), X86Operand::RipRelLabel(name.clone())));
                    X86Operand::Reg(X86Reg::R10)
                } else {
                    self.operand_to_op(left)
                };

                let r_op = if let Operand::Var(var) = right {
                    if let Some(off) = self.alloca_buffers.get(var) {
                         self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::R11), X86Operand::Mem(X86Reg::Rbp, *off)));
                         X86Operand::Reg(X86Reg::R11)
                    } else { self.operand_to_op(right) }
                } else if let Operand::Global(name) = right {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::R11), X86Operand::RipRelLabel(name.clone())));
                    X86Operand::Reg(X86Reg::R11)
                } else {
                    self.operand_to_op(right)
                };

                let d_op = self.var_to_op(*dest);
                InstructionGenerator::gen_binary_op(&mut self.asm, *dest, op, l_op, r_op, d_op);
            }
            IrInstruction::FloatBinary { dest, op, left, right } => {
                gen_float_binary_op(self, *dest, op, left, right);
            }
            IrInstruction::Unary { dest, op, src } => {
                let s_op = self.operand_to_op(src);
                let d_op = self.var_to_op(*dest);
                InstructionGenerator::gen_unary_op(&mut self.asm, *dest, op, s_op, d_op);
            }
            IrInstruction::FloatUnary { dest, op, src } => {
                gen_float_unary_op(self, *dest, op, src);
            }
            IrInstruction::Phi { .. } => {}
            IrInstruction::Alloca { dest, r#type } => {
                self.var_types.insert(*dest, Type::Pointer(Box::new(r#type.clone())));
            }
            IrInstruction::Load { dest, addr, value_type } => {
                self.gen_load(*dest, addr, value_type);
            }
            IrInstruction::Store { addr, src, value_type } => {
                self.gen_store(addr, src, value_type);
            }
            IrInstruction::GetElementPtr { dest, base, index, element_type } => {
                self.gen_gep(*dest, base, index, element_type);
            }
            IrInstruction::Call { dest, name, args } => {
                self.gen_call(dest, name, args);
            }
            IrInstruction::IndirectCall { dest, func_ptr, args } => {
                self.gen_indirect_call(dest, func_ptr, args);
            }
            IrInstruction::InlineAsm { template, outputs, inputs, clobbers, is_volatile } => {
                self.gen_inline_asm(template, outputs, inputs, clobbers, *is_volatile);
            }
        }
    }


    fn gen_load(&mut self, dest: VarId, addr: &Operand, value_type: &model::Type) {
        self.var_types.insert(dest, value_type.clone());
        let d_op = self.var_to_op(dest);
        let is_float = matches!(value_type, Type::Float | Type::Double);
        let use_dword = matches!(value_type, Type::Int | Type::Float);
        
        // Optimization: if loading directly from an alloca (stack slot), just read it
        if let Operand::Var(var) = addr {
             if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                 if is_float {
                     self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, *buffer_offset)));
                     self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                 } else {
                     if use_dword {
                         self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rbp, *buffer_offset)));
                         self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                     } else {
                         self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                     }
                     self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                 }
                 return;
             }
        }

        // Optimization: if loading directly from a global, use RIP-relative load
        if let Operand::Global(name) = addr {
             if is_float {
                 self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::GlobalMem(name.clone())));
                 self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
             } else {
                 if use_dword {
                     // 32-bit int: Mov EAX, [rip+name]
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::GlobalMem(name.clone())));
                     self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                 } else {
                     // 64-bit int/ptr: Mov RAX, [rip+name]
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                 }
                 self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
             }
             return;
        }
        
        // General case: Load address into RAX, then dereference
        if let Operand::Var(var) = addr {
             let v_op = self.var_to_op(*var);
             self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), v_op));
        } else {
             let op = self.operand_to_op(addr);
             match op {
                 X86Operand::Label(l) | X86Operand::RipRelLabel(l) => {
                     self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l)));
                 }
                 _ => {
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), op));
                 }
             }
        }
        
        if is_float {
            self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rax, 0)));
            self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
        } else {
             if use_dword {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
                 self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
             } else {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
             }
             self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }

    fn gen_store(&mut self, addr: &Operand, src: &Operand, value_type: &model::Type) {
        let is_float = matches!(value_type, Type::Float | Type::Double);
        let use_dword = matches!(value_type, Type::Int | Type::Float);

        // Load src into register
        if is_float {
            let s_op = self.operand_to_op(src);
             match s_op {
                 X86Operand::Reg(X86Reg::Xmm0) => {},
                 _ => self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op))
             }
        } else {
            let s_op = self.operand_to_op(src);
             // Handle alloca address loading vs value loading
             if let Operand::Global(name) = src {
                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::RipRelLabel(name.clone())));
             } else if let Operand::Var(v) = src {
                 if let Some(off) = self.alloca_buffers.get(v) {
                     self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::Mem(X86Reg::Rbp, *off)));
                 } else {
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
                 }
             } else {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
             }
        }

        // Load address into RAX
        if let Operand::Var(var) = addr {
             if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                  self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
             } else {
                 let b_op = self.var_to_op(*var);
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
             }
        } else {
             let op = self.operand_to_op(addr);
             if let X86Operand::Label(l) = &op {
                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l.clone())));
             } else if let X86Operand::RipRelLabel(l) = &op {
                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l.clone())));
             } else {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), op));
             }
        }
        
        // Store
        if is_float {
            self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
        } else if use_dword {
            self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
        } else {
            self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
        }
    }

    fn gen_gep(&mut self, dest: VarId, base: &Operand, index: &Operand, element_type: &Type) {
        let i_op = self.operand_to_op(index);
        let d_op = self.var_to_op(dest);
        let elem_size = self.get_type_size(element_type) as i64;
        
        match base {
            Operand::Global(name) => {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
            }
            Operand::Var(var) => {
                if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                } else {
                    let b_op = self.var_to_op(*var);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
                }
            }
            _ => {}
        }

        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), i_op));
        if elem_size != 1 {
            self.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rcx), X86Operand::Imm(elem_size)));
        }
        self.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    }

    fn gen_call(&mut self, dest: &Option<VarId>, name: &str, args: &[Operand]) {
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
        let float_regs = [X86Reg::Xmm0, X86Reg::Xmm1, X86Reg::Xmm2, X86Reg::Xmm3];
        
        for (i, arg) in args.iter().enumerate() {
            let is_float = match arg {
                Operand::FloatConstant(_) => true,
                Operand::Var(v) => {
                    self.var_types.get(v).map_or(false, |t| matches!(t, Type::Float | Type::Double))
                }
                _ => false,
            };
            
            if i < 4 {
                if is_float {
                    let label = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(float_regs[i].clone()), label));
                } else {
                    let mut handled = false;
                    if let Operand::Var(var) = arg {
                         if let Some(off) = self.alloca_buffers.get(var) {
                             self.asm.push(X86Instr::Lea(X86Operand::Reg(param_regs[i].clone()), X86Operand::Mem(X86Reg::Rbp, *off)));
                             handled = true;
                         }
                    }
                    if !handled {
                        if let Operand::Global(name) = arg {
                            self.asm.push(X86Instr::Lea(X86Operand::Reg(param_regs[i].clone()), X86Operand::RipRelLabel(name.clone())));
                        } else {
                            let val = self.operand_to_op(arg);
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(param_regs[i].clone()), val));
                        }
                    }
                }
            } else {
                let offset = 32 + (i - 4) * 8;
                if is_float {
                    let label = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    let mut handled = false;
                    if let Operand::Var(var) = arg {
                         if let Some(off) = self.alloca_buffers.get(var) {
                             self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *off)));
                             handled = true;
                         }
                    }
                    if !handled {
                        if let Operand::Global(name) = arg {
                            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                        } else {
                            let val = self.operand_to_op(arg);
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                        }
                    }
                    self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
                }
            }
        }
        
        self.asm.push(X86Instr::Call(name.to_string()));
        
        if let Some(d) = dest {
            let returns_float = self.func_return_types.get(name)
                .map_or(false, |ret_type| matches!(ret_type, Type::Float | Type::Double));
            
            if returns_float {
                self.var_types.insert(*d, Type::Float);
            }
            
            let d_op = self.var_to_op(*d);
            if returns_float {
                self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }

    fn gen_indirect_call(&mut self, dest: &Option<VarId>, func_ptr: &Operand, args: &[Operand]) {
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
        let float_regs = [X86Reg::Xmm0, X86Reg::Xmm1, X86Reg::Xmm2, X86Reg::Xmm3];
        
        let fp_op = self.operand_to_op(func_ptr);
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), fp_op));
        
        for (i, arg) in args.iter().enumerate() {
            let is_float = match arg {
                Operand::FloatConstant(_) => true,
                Operand::Var(v) => {
                    self.var_types.get(v).map_or(false, |t| matches!(t, Type::Float | Type::Double))
                }
                _ => false,
            };

            if i < 4 {
                if is_float {
                    let label = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(float_regs[i].clone()), label));
                } else {
                    let val = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(param_regs[i].clone()), val));
                }
            } else {
                let offset = 32 + (i - 4) * 8;
                if is_float {
                    let label = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    let val = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                    self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
                }
            }
        }
        
        self.asm.push(X86Instr::CallIndirect(X86Operand::Reg(X86Reg::R10)));
        
        if let Some(d) = dest {
             let mut is_float_ret = false;
             
             // Try to infer return type from function pointer type
             if let Operand::Var(v) = func_ptr {
                 if let Some(t) = self.var_types.get(v) {
                     if let Type::FunctionPointer { return_type, .. } = t {
                         if matches!(**return_type, Type::Float | Type::Double) {
                             is_float_ret = true;
                         }
                         // Store the inferred type for the destination variable
                         self.var_types.insert(*d, *return_type.clone());
                     }
                 }
             }

            // Fallback to checking destination type if already known
            if !is_float_ret {
                if let Some(t) = self.var_types.get(d) {
                    if matches!(t, Type::Float | Type::Double) {
                        is_float_ret = true;
                    }
                }
            }
            
            if is_float_ret {
                self.var_types.insert(*d, Type::Float);
                let dest_op = self.var_to_op(*d);
                self.asm.push(X86Instr::Movss(dest_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                let dest_op = self.var_to_op(*d);
                self.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }

    fn gen_inline_asm(&mut self, template: &str, outputs: &[VarId], inputs: &[Operand], _clobbers: &[String], _is_volatile: bool) {
        let mut asm_code = template.to_string();
        for (i, output_var) in outputs.iter().enumerate() {
            let placeholder = format!("%{}", i);
            let output_op = self.var_to_op(*output_var);
            let operand_str = match &output_op {
                X86Operand::Mem(reg, offset) => {
                    if *offset == 0 { format!("DWORD PTR [{}]", reg.to_str()) } 
                    else { format!("DWORD PTR [{}{}]", reg.to_str(), if *offset > 0 { format!("+{}", offset) } else { offset.to_string() }) }
                }
                X86Operand::Reg(r) => r.to_str().to_string(),
                _ => "eax".to_string(),
            };
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        for (i, input_op) in inputs.iter().enumerate() {
            let placeholder = format!("%{}", outputs.len() + i);
            let input_x86_op = self.operand_to_op(input_op);
            let operand_str = match &input_x86_op {
                X86Operand::Imm(val) => val.to_string(),
                X86Operand::Reg(r) => r.to_str().to_string(),
                X86Operand::Mem(reg, offset) => {
                    if *offset == 0 { format!("DWORD PTR [{}]", reg.to_str()) } 
                    else { format!("DWORD PTR [{}{}]", reg.to_str(), if *offset > 0 { format!("+{}", offset) } else { offset.to_string() }) }
                }
                _ => "0".to_string(),
            };
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        asm_code = asm_code.replace("{$}", "");
        self.asm.push(X86Instr::Raw(asm_code));
    }

    fn get_or_create_slot(&mut self, var: VarId) -> i32 {
        if let Some(slot) = self.stack_slots.get(&var) {
            return *slot;
        }
        self.next_slot += 8;
        let slot = -self.next_slot;
        self.stack_slots.insert(var, slot);
        slot
    }

    pub(crate) fn var_to_op(&mut self, var: VarId) -> X86Operand {
        if let Some(&buffer_offset) = self.alloca_buffers.get(&var) {
            return X86Operand::Mem(X86Reg::Rbp, buffer_offset);
        }
        if let Some(var_type) = self.var_types.get(&var) {
            if matches!(var_type, Type::Float | Type::Double) {
                let slot = self.get_or_create_slot(var);
                return X86Operand::FloatMem(X86Reg::Rbp, slot);
            }
        }
        if let Some(reg) = self.reg_alloc.get(&var) {
            return X86Operand::Reg(reg.to_x86());
        }
        let slot = self.get_or_create_slot(var);
        X86Operand::Mem(X86Reg::Rbp, slot)
    }

    pub(crate) fn operand_to_op(&mut self, op: &Operand) -> X86Operand {
        match op {
            Operand::Constant(c) => X86Operand::Imm(*c),
            Operand::FloatConstant(f) => {
                let label = self.get_or_create_float_const(*f);
                X86Operand::RipRelLabel(label)
            }
            Operand::Var(v) => self.var_to_op(*v),
            Operand::Global(s) => X86Operand::Label(s.clone()),
        }
    }
    
    pub(crate) fn get_or_create_float_const(&mut self, value: f64) -> String {
        for (label, &v) in self.float_constants.iter() {
            if (v - value).abs() < f64::EPSILON {
                return label.clone();
            }
        }
        let label = format!(".LC{}", self.next_float_const);
        *self.next_float_const += 1;
        self.float_constants.insert(label.clone(), value);
        label
    }

    fn get_type_size(&self, r#type: &model::Type) -> usize {
        let calculator = TypeCalculator::new(self.structs, self.unions);
        calculator.get_type_size(r#type)
    }

    fn get_current_block_id(&self) -> BlockId {
        for instr in self.asm.iter().rev() {
            if let X86Instr::Label(l) = instr {
                if let Some(pos) = l.rfind('_') {
                    if let Ok(id) = l[pos+1..].parse::<usize>() {
                        return BlockId(id);
                    }
                }
            }
        }
        BlockId(0)
    }

    fn resolve_phis(&mut self, target: BlockId, from: BlockId, func: &IrFunction) {
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
    
    fn gen_terminator(&mut self, term: &IrTerminator, func_name: &str, func: &IrFunction) {
        match term {
            IrTerminator::Ret(op) => {
                if let Some(o) = op {
                    let is_float_return = matches!(func.return_type, Type::Float | Type::Double);
                    if is_float_return {
                        let label = self.operand_to_op(o);
                        self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                    } else {
                        let val = self.operand_to_op(o);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                    }
                }
                
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
