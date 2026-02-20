use std::collections::HashMap;
use crate::x86::{X86Reg, X86Operand, X86Instr};
use model::Type;
use ir::{Function as IrFunction, BlockId, VarId, Operand, Instruction as IrInstruction, Terminator as IrTerminator};
use crate::regalloc::{PhysicalReg, allocate_registers};
use crate::instructions::InstructionGenerator;
use crate::types::TypeCalculator;
use crate::float_ops::{gen_float_binary_op, gen_float_unary_op};
use crate::memory_ops::{gen_load, gen_store, gen_gep};
use crate::call_ops::{gen_call, gen_indirect_call};
use crate::calling_convention::get_convention;

/// Handles generation of code for a single function
pub struct FunctionGenerator<'a> {
    pub asm: Vec<X86Instr>,
    
    // Context from parent Codegen
    pub(crate) structs: &'a HashMap<String, model::StructDef>,
    pub(crate) unions: &'a HashMap<String, model::UnionDef>,
    pub(crate) func_return_types: &'a HashMap<String, Type>,
    pub(crate) float_constants: &'a mut HashMap<String, f64>,
    pub(crate) next_float_const: &'a mut usize,
    pub(crate) target: &'a model::TargetConfig,
    
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
        target: &'a model::TargetConfig,
    ) -> Self {
        Self {
            asm: Vec::new(),
            structs,
            unions,
            func_return_types,
            float_constants,
            next_float_const,
            target,
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
        // Check if function is variadic (uses va_start)
        let uses_va_start = func.blocks.iter().any(|b| b.instructions.iter().any(|i| matches!(i, IrInstruction::VaStart {..})));

        // Get calling convention for this target
        let convention = get_convention(self.target.calling_convention);
        
        // Perform register allocation
        if self.enable_regalloc {
            self.reg_alloc = allocate_registers(func, self.target);
        }
        
        // Identify used callee-saved registers
        self.current_saved_regs.clear();
        let used_regs: std::collections::HashSet<_> = self.reg_alloc.values().collect();
        for reg in PhysicalReg::callee_saved(self.target) {
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
        
        // Insert a placeholder Sub(Rsp) instruction that will be backpatched
        // after code generation, when the final stack size is known.
        // This is necessary because resolve_phis and gen_instr may create
        // additional stack slots beyond what allocate_stack_slots predicts.
        let sub_rsp_index = self.asm.len();
        self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rsp), X86Operand::Imm(0))); // placeholder

        let shadow_space = convention.shadow_space_size() as i32;

        // Spill register parameters to shadow space if variadic
        if uses_va_start {
            for (i, reg) in convention.param_regs().iter().enumerate() {
                let offset = 16 + (i * 8) as i32;
                self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rbp, offset), X86Operand::Reg(reg.clone())));
            }
        }

        // Handle parameters
        let param_regs = convention.param_regs();
        let float_regs = convention.float_param_regs();
        
        // Build a list of (source_reg, dest_op) pairs to handle conflicts
        let mut param_moves: Vec<(X86Operand, X86Operand, bool)> = Vec::new();
        
        for (i, (param_type, var)) in func.params.iter().enumerate() {
            // Record parameter type for later use
            self.var_types.insert(*var, param_type.clone());
            
            // Parameters always go to their stack slots, not registers
            // This ensures they're preserved across function calls that clobber parameter registers
            let dest = if let Some(&buffer_offset) = self.alloca_buffers.get(var) {
                X86Operand::Mem(X86Reg::Rbp, buffer_offset)
            } else if let Some(var_type) = self.var_types.get(var) {
                if matches!(var_type, Type::Float | Type::Double) {
                    let slot = self.stack_slots.get(var).copied().unwrap_or_else(|| self.get_or_create_slot(*var));
                    X86Operand::FloatMem(X86Reg::Rbp, slot)
                } else {
                    let slot = self.stack_slots.get(var).copied().unwrap_or_else(|| self.get_or_create_slot(*var));
                    X86Operand::Mem(X86Reg::Rbp, slot)
                }
            } else {
                let slot = self.stack_slots.get(var).copied().unwrap_or_else(|| self.get_or_create_slot(*var));
                X86Operand::Mem(X86Reg::Rbp, slot)
            };
            
            let is_float = matches!(param_type, Type::Float | Type::Double);
            
            if i < param_regs.len() {
                if is_float && i < float_regs.len() {
                    // Float parameters come in XMM registers
                    let src = X86Operand::Reg(float_regs[i].clone());
                    if src != dest {
                        param_moves.push((src, dest, true));
                    }
                } else if !is_float {
                    // Integer/pointer parameters come in general-purpose registers
                    let src = X86Operand::Reg(param_regs[i].clone());
                    if src != dest {
                        param_moves.push((src, dest, false));
                    }
                }
            } else {
                // Parameters beyond register count are on the stack
                let offset = 16 + shadow_space + ((i - param_regs.len()) * 8) as i32;
                if is_float {
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Movss(dest, X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(X86Reg::Rax)));
                }
            }
        }
        
        // Execute parameter moves, handling conflicts by breaking cycles
        let mut completed = vec![false; param_moves.len()];
        
        while completed.iter().any(|&c| !c) {
            let mut made_progress = false;
            
            for i in 0..param_moves.len() {
                if completed[i] {
                    continue;
                }
                
                let (ref src, ref dst, is_float) = param_moves[i];
                
                // Check if dst conflicts with any uncompleted src
                let has_conflict = param_moves.iter().enumerate().any(|(j, (s, _, _))| {
                    !completed[j] && i != j && dst == s
                });
                
                if !has_conflict {
                    // Safe to move
                    if is_float {
                        self.asm.push(X86Instr::Movss(dst.clone(), src.clone()));
                    } else {
                        self.asm.push(X86Instr::Mov(dst.clone(), src.clone()));
                    }
                    completed[i] = true;
                    made_progress = true;
                }
            }
            
            // If we couldn't make progress, we have a cycle - break it with a swap
            if !made_progress {
                // Find the cycle
                for i in 0..param_moves.len() {
                    if completed[i] {
                        continue;
                    }
                    
                    let (ref src_i, ref dst_i, is_float_i) = param_moves[i];
                    
                    // Look for the other move in the cycle (where dst_i == src_j)
                    for j in 0..param_moves.len() {
                        if i == j || completed[j] {
                            continue;
                        }
                        
                        let (ref src_j, ref dst_j, is_float_j) = param_moves[j];
                        
                        if dst_i == src_j && src_i == dst_j {
                            // Found a 2-cycle: swap regi <-> regj
                            // Standard 3-instruction swap: temp = src_i; dst_j = src_j; dst_i = temp
                            assert_eq!(is_float_i, is_float_j, "Float/int mismatch in cycle");
                            
                            if is_float_i {
                                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm7), src_i.clone()));
                                self.asm.push(X86Instr::Movss(dst_j.clone(), src_j.clone()));
                                self.asm.push(X86Instr::Movss(dst_i.clone(), X86Operand::Reg(X86Reg::Xmm7)));
                            } else {
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), src_i.clone()));
                                self.asm.push(X86Instr::Mov(dst_j.clone(), src_j.clone()));
                                self.asm.push(X86Instr::Mov(dst_i.clone(), X86Operand::Reg(X86Reg::R10)));
                            }
                            completed[i] = true;
                            completed[j] = true;
                            made_progress = true;
                            break;
                        }
                    }
                    
                    if made_progress {
                        break;
                    }
                }
                
                // If still no progress, we may have a longer cycle (3+) - shouldn't happen with simple parameter passing
                // but handle by breaking one edge
                if !made_progress {
                    for i in 0..param_moves.len() {
                        if !completed[i] {
                            let (ref src, ref dst, is_float) = param_moves[i];
                            if is_float {
                                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm7), src.clone()));
                                self.asm.push(X86Instr::Movss(dst.clone(), X86Operand::Reg(X86Reg::Xmm7)));
                            } else {
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), src.clone()));
                                self.asm.push(X86Instr::Mov(dst.clone(), X86Operand::Reg(X86Reg::R10)));
                            }
                            completed[i] = true;
                            break;
                        }
                    }
                }
            }
        }
        
        // Remove parameters from reg_alloc since they're now in stack slots
        // This ensures var_to_op returns the stack slot, not the clobbered parameter register
        for (_, var) in &func.params {
            self.reg_alloc.remove(var);
        }

        for block in &func.blocks {
            // Skip unreachable blocks (marked by CFG simplification)
            if block.instructions.is_empty() && matches!(block.terminator, IrTerminator::Unreachable) {
                continue;
            }
            
            self.asm.push(X86Instr::Label(format!("{}_{}", func.name, block.id.0)));
            for inst in &block.instructions {
                self.gen_instr(inst);
            }
            self.gen_terminator(&block.terminator, &func.name, func);
        }

        // Backpatch the Sub(Rsp) placeholder with the final stack size,
        // now that all stack slots have been allocated during code generation.
        let saved_size = (self.current_saved_regs.len() * 8) as i32;
        let locals_size = self.next_slot - saved_size;
        let shadow_space = convention.shadow_space_size() as i32;
        
        // Compute maximum stack space needed for outgoing call arguments
        let num_param_regs = convention.param_regs().len();
        let max_call_stack_args = func.blocks.iter()
            .flat_map(|b| b.instructions.iter())
            .filter_map(|inst| match inst {
                IrInstruction::Call { args, .. } => Some(args.len()),
                IrInstruction::IndirectCall { args, .. } => Some(args.len()),
                _ => None,
            })
            .map(|n| if n > num_param_regs { ((n - num_param_regs) * 8) as i32 } else { 0 })
            .max()
            .unwrap_or(0);
        
        let total_stack = saved_size + locals_size + shadow_space + max_call_stack_args;
        let aligned_total = (total_stack + 15) & !15;
        let sub_amount = aligned_total - saved_size;
        
        if sub_amount > 0 {
            self.asm[sub_rsp_index] = X86Instr::Sub(X86Operand::Reg(X86Reg::Rsp), X86Operand::Imm(sub_amount as i64));
        } else {
            // Replace with a no-op (empty raw string that produces nothing)
            self.asm[sub_rsp_index] = X86Instr::Raw(String::new());
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
                    IrInstruction::GetElementPtr { dest, .. } |
                    IrInstruction::VaArg { dest, .. } => {
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
                    IrInstruction::Store { .. } | IrInstruction::VaStart { .. } |
                    IrInstruction::VaEnd { .. } | IrInstruction::VaCopy { .. } => {}
                }
            }
        }
        
        // Parameters: always allocate stack slots
        // Even if register-allocated, parameters need stack homes because
        // their parameter-passing registers (rdi, rsi, etc.) get clobbered by function calls
        for (_, var) in &func.params {
            self.get_or_create_slot(*var);
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
                         // Check operand sizes to avoid invalid mov instructions
                         let src_is_dword = matches!(s_op, X86Operand::DwordMem(..));
                         let dst_is_dword = matches!(d_op, X86Operand::DwordMem(..));
                         
                         if src_is_dword && dst_is_dword {
                             // Both 32-bit
                             self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                             self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                         } else if src_is_dword {
                             // 32-bit source to 64-bit dest - zero extend via EAX
                             self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                             self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                         } else if dst_is_dword {
                             // 64-bit source to 32-bit dest - truncate via RAX->EAX
                             self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                             self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                         } else {
                             // Both 64-bit
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
                } else if matches!(s_op, X86Operand::DwordMem(..)) && matches!(d_op, X86Operand::DwordMem(..)) {
                     // Both 32-bit memory
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                     self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                } else if matches!(s_op, X86Operand::Mem(..)) && matches!(d_op, X86Operand::Mem(..)) {
                     // Both 64-bit memory
                     self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                     self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                } else {
                     // Handle all other cases including register<->memory moves
                     let src_is_dword = matches!(s_op, X86Operand::DwordMem(..));
                     let dst_is_dword = matches!(d_op, X86Operand::DwordMem(..));
                     let dst_is_reg64 = matches!(d_op, X86Operand::Reg(X86Reg::Rax | X86Reg::Rbx | X86Reg::Rcx | X86Reg::Rdx | X86Reg::Rsi | X86Reg::Rdi | X86Reg::Rsp | X86Reg::Rbp | X86Reg::R8 | X86Reg::R9 | X86Reg::R10 | X86Reg::R11 | X86Reg::R12 | X86Reg::R13 | X86Reg::R14 | X86Reg::R15));
                     let dst_is_reg32 = matches!(d_op, X86Operand::Reg(X86Reg::Eax | X86Reg::Ebx | X86Reg::Ecx | X86Reg::Edx | X86Reg::Esi | X86Reg::Edi | X86Reg::Esp | X86Reg::Ebp | X86Reg::R8d | X86Reg::R9d | X86Reg::R10d | X86Reg::R11d | X86Reg::R12d | X86Reg::R13d | X86Reg::R14d | X86Reg::R15d));
                     
                     if src_is_dword && dst_is_reg64 {
                         // 32-bit memory to 64-bit register - need to go through EAX first
                         self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                         self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                     } else if src_is_dword && !dst_is_dword && !dst_is_reg32 {
                         //32-bit source to non-32-bit dest (64-bit mem or reg)
                         self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                         self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                     } else if !src_is_dword && dst_is_dword {
                         // 64-bit source to 32-bit dest
                         self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                         self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                     } else {
                         self.asm.push(X86Instr::Mov(d_op, s_op));
                     }
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
                // Determine signedness for shift direction: unsigned types use shr, signed use sar
                let is_signed = if let Operand::Var(lv) = left {
                    !matches!(self.var_types.get(lv), Some(model::Type::UnsignedInt | model::Type::UnsignedChar | model::Type::UnsignedShort | model::Type::UnsignedLong | model::Type::UnsignedLongLong))
                } else {
                    true // literals and globals default to signed
                };
                InstructionGenerator::gen_binary_op(&mut self.asm, *dest, op, l_op, r_op, d_op, is_signed);
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
                gen_load(self, *dest, addr, value_type);
            }
            IrInstruction::Store { addr, src, value_type } => {
                gen_store(self, addr, src, value_type);
            }
            IrInstruction::GetElementPtr { dest, base, index, element_type } => {
                gen_gep(self, *dest, base, index, element_type);
            }
            IrInstruction::Call { dest, name, args } => {
                gen_call(self, dest, name, args);
            }
            IrInstruction::IndirectCall { dest, func_ptr, args } => {
                gen_indirect_call(self, dest, func_ptr, args);
            }
            IrInstruction::InlineAsm { template, outputs, inputs, clobbers, is_volatile } => {
                self.gen_inline_asm(template, outputs, inputs, clobbers, *is_volatile);
            }
            IrInstruction::VaStart { list, arg_index } => {
                // Windows x64: Arguments are contiguous in stack (Shadow Space + Stack Args)
                // Next param is at index + 1
                // Offset = 16 (return addr + saved RBP) + (arg_index + 1) * 8
                let offset = 16 + (arg_index + 1) * 8;
                
                // LEA RAX, [RBP + offset]
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset as i32)));
                
                // Store RAX into *list. list is Operand::Var(alloca). var_to_op is [RBP - offset].
                // So MOV [RBP - offset], RAX
                // Since `list` is char*, and var_to_op returns the location of that variable (e.g. pointer on stack)
                let list_dest = self.operand_to_op(list);
                self.asm.push(X86Instr::Mov(list_dest, X86Operand::Reg(X86Reg::Rax)));
            }
            IrInstruction::VaEnd { .. } => {
                // No-op for now
            }
            IrInstruction::VaCopy { dest, src } => {
                let s_op = self.operand_to_op(src);
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                let d_op = self.operand_to_op(dest);
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            IrInstruction::VaArg { .. } => {
                // Needs implementation if __builtin_va_arg is used
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

    pub(crate) fn get_type_size(&self, r#type: &model::Type) -> usize {
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
                                  // Handle size mismatch between source and dest
                                  let src_is_dword = matches!(s_op, X86Operand::DwordMem(..));
                                  let dst_is_dword = matches!(d_op, X86Operand::DwordMem(..));
                                  
                                  if src_is_dword && dst_is_dword {
                                      self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                                      self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
                                  } else if src_is_dword {
                                      // 32-bit source to 64-bit dest
                                      self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                                      self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                                  } else if dst_is_dword {
                                      // 64-bit source to 32-bit dest
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
                        // Handle 32-bit vs 64-bit return values
                        match val {
                            X86Operand::DwordMem(..) => {
                                // 32-bit memory operand - load into EAX, then zero-extend to RAX implicitly
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), val));
                            }
                            X86Operand::Imm(i) if i >= i32::MIN as i64 && i <= i32::MAX as i64 => {
                                // Small immediate - can use EAX
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), val));
                            }
                            _ => {
                                // 64-bit operand or large immediate
                                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
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
