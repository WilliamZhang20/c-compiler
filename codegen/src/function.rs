use std::collections::HashMap;
use crate::x86::{X86Reg, X86Operand, X86Instr};
use model::Type;
use ir::{Function as IrFunction, VarId, BlockId, Operand, Instruction as IrInstruction, Terminator as IrTerminator, SimdOp};
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
    pub(crate) float_constants: &'a mut HashMap<String, (f64, bool)>,
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
    pub(crate) current_block: BlockId,
    /// Maps IR VarIds to XMM/YMM register indices for vector operations
    pub(crate) simd_reg_map: HashMap<VarId, u8>,
    pub(crate) next_simd_reg: u8,
    /// Offset from RBP to the start of the register save area (for variadic functions)
    pub(crate) va_save_area_offset: Option<i32>,
    /// Next synthetic VarId for codegen-generated temporaries
    pub(crate) next_temp_var: usize,
    pub(crate) profile_generate: bool,
    pub(crate) profile_counters: Option<&'a mut Vec<String>>,
}

impl<'a> FunctionGenerator<'a> {
    /// Get the calling convention for this target.
    pub(crate) fn convention(&self) -> Box<dyn crate::calling_convention::CallingConvention> {
        get_convention(self.target.calling_convention)
    }

    pub fn new(
        structs: &'a HashMap<String, model::StructDef>,
        unions: &'a HashMap<String, model::UnionDef>,
        func_return_types: &'a HashMap<String, Type>,
        float_constants: &'a mut HashMap<String, (f64, bool)>,
        next_float_const: &'a mut usize,
        enable_regalloc: bool,
        target: &'a model::TargetConfig,
        profile_generate: bool,
        profile_counters: Option<&'a mut Vec<String>>,
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
            current_block: BlockId(0),
            simd_reg_map: HashMap::new(),
            next_simd_reg: 0,
            va_save_area_offset: None,
            next_temp_var: 100_000,
            profile_generate,
            profile_counters,
        }
    }

    pub fn gen_function(mut self, func: &IrFunction) -> Vec<X86Instr> {
        // Seed var_types from IR-level type annotations (e.g. mem2reg phi vars)
        for (var, ty) in &func.var_types {
            self.var_types.insert(*var, ty.clone());
        }

        // Check if function is variadic (uses va_start)
        let uses_va_start = func.blocks.iter().any(|b| b.instructions.iter().any(|i| matches!(i, IrInstruction::VaStart {..})));

        // Get calling convention for this target
        let convention = self.convention();
        
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
        
        // CFI: start procedure
        if matches!(self.target.platform, model::Platform::Linux) {
            self.asm.push(X86Instr::Raw(".cfi_startproc".to_string()));
        }
        
        // Prologue
        self.asm.push(X86Instr::Push(X86Reg::Rbp));
        if matches!(self.target.platform, model::Platform::Linux) {
            self.asm.push(X86Instr::Raw(".cfi_def_cfa_offset 16".to_string()));
            self.asm.push(X86Instr::Raw(".cfi_offset rbp, -16".to_string()));
        }
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rbp), X86Operand::Reg(X86Reg::Rsp)));
        if matches!(self.target.platform, model::Platform::Linux) {
            self.asm.push(X86Instr::Raw(".cfi_def_cfa_register rbp".to_string()));
        }
        
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

        // Spill register parameters to a local save area if variadic.
        // On SysV AMD64, we allocate 48 bytes (6 GP regs × 8) below the frame.
        if uses_va_start {
            // Allocate save area slots
            let save_base = self.next_slot;
            for (i, reg) in convention.param_regs().iter().enumerate() {
                let slot_offset = save_base + (i * 8) as i32 + 8;
                self.asm.push(X86Instr::Mov(
                    X86Operand::Mem(X86Reg::Rbp, -(slot_offset as i32)),
                    X86Operand::Reg(reg.clone())));
            }
            // Record the save area base for va_start to reference
            self.va_save_area_offset = Some(save_base + 8);
            self.next_slot += (convention.param_regs().len() * 8) as i32;
        }

        // Handle parameters
        let param_regs = convention.param_regs();
        let float_regs = convention.float_param_regs();
        
        // Build a list of (source_reg, dest_op) pairs to handle conflicts
        let mut param_moves: Vec<(X86Operand, X86Operand, bool)> = Vec::new();
        
        // Track actual register index (struct params may consume >1 register)
        let mut reg_idx = 0usize;
        let mut float_reg_idx = 0usize;
        
        for (_i, (param_type, var)) in func.params.iter().enumerate() {
            // Record parameter type for later use
            self.var_types.insert(*var, param_type.clone());
            
            // Check if this is a struct parameter passed by value
            let struct_class = crate::call_ops::classify_struct_arg(&self, param_type);
            
            if let Some(class) = struct_class {
                // Struct parameter — assemble register values into the alloca buffer
                let buffer_offset = self.alloca_buffers.get(var).copied()
                    .unwrap_or_else(|| {
                        let slot = self.get_or_create_slot(*var);
                        slot
                    });
                
                match class {
                    crate::call_ops::StructArgClass::OneReg => {
                        if reg_idx < param_regs.len() {
                            param_moves.push((
                                X86Operand::Reg(param_regs[reg_idx].clone()),
                                X86Operand::Mem(X86Reg::Rbp, buffer_offset),
                                false,
                            ));
                            reg_idx += 1;
                        } else {
                            // From stack
                            let offset = 16 + shadow_space + ((reg_idx - param_regs.len()) * 8) as i32;
                            self.asm.push(X86Instr::Mov(
                                X86Operand::Reg(X86Reg::Rax),
                                X86Operand::Mem(X86Reg::Rbp, offset),
                            ));
                            self.asm.push(X86Instr::Mov(
                                X86Operand::Mem(X86Reg::Rbp, buffer_offset),
                                X86Operand::Reg(X86Reg::Rax),
                            ));
                            reg_idx += 1;
                        }
                    }
                    crate::call_ops::StructArgClass::TwoReg => {
                        // First eightbyte
                        if reg_idx < param_regs.len() {
                            param_moves.push((
                                X86Operand::Reg(param_regs[reg_idx].clone()),
                                X86Operand::Mem(X86Reg::Rbp, buffer_offset),
                                false,
                            ));
                        }
                        reg_idx += 1;
                        // Second eightbyte
                        if reg_idx < param_regs.len() {
                            param_moves.push((
                                X86Operand::Reg(param_regs[reg_idx].clone()),
                                X86Operand::Mem(X86Reg::Rbp, buffer_offset + 8),
                                false,
                            ));
                        }
                        reg_idx += 1;
                    }
                    crate::call_ops::StructArgClass::Memory => {
                        // Large struct → pointer was passed in register
                        if reg_idx < param_regs.len() {
                            param_moves.push((
                                X86Operand::Reg(param_regs[reg_idx].clone()),
                                X86Operand::Mem(X86Reg::Rbp, self.stack_slots.get(var).copied()
                                    .unwrap_or_else(|| self.get_or_create_slot(*var))),
                                false,
                            ));
                        }
                        reg_idx += 1;
                    }
                }
                // Struct params live in memory (alloca buffer) for field access,
                // so remove any GP register allocation the register allocator
                // may have assigned — var_to_op must return the memory slot.
                self.reg_alloc.remove(var);
                continue;
            }
            
            // Non-struct parameter handling
            // For non-float params with a register allocation, store directly
            // to the allocated register to avoid redundant stack spills.
            // Float params always use stack slots since the GP register allocator
            // doesn't handle XMM registers.
            let is_float = matches!(param_type, Type::Float | Type::Double);
            let dest = if let Some(&buffer_offset) = self.alloca_buffers.get(var) {
                X86Operand::Mem(X86Reg::Rbp, buffer_offset)
            } else if !is_float {
                if let Some(phys) = self.reg_alloc.get(var) {
                    // Non-float parameter has a GP register — store directly to it
                    X86Operand::Reg(phys.to_x86())
                } else {
                    let slot = self.stack_slots.get(var).copied().unwrap_or_else(|| self.get_or_create_slot(*var));
                    X86Operand::Mem(X86Reg::Rbp, slot)
                }
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
            
            // For float params that got GP register assignments, remove from
            // reg_alloc so var_to_op returns the float stack slot instead
            if is_float {
                self.reg_alloc.remove(var);
            }
            
            if is_float {
                if float_reg_idx < float_regs.len() {
                    let src = X86Operand::Reg(float_regs[float_reg_idx].clone());
                    if src != dest {
                        param_moves.push((src, dest, true));
                    }
                    float_reg_idx += 1;
                }
                reg_idx += 1;
            } else if reg_idx < param_regs.len() {
                let src = X86Operand::Reg(param_regs[reg_idx].clone());
                if src != dest {
                    param_moves.push((src, dest, false));
                }
                reg_idx += 1;
            } else {
                // Parameters beyond register count are on the stack
                let offset = 16 + shadow_space + ((reg_idx - param_regs.len()) * 8) as i32;
                if is_float {
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Movss(dest, X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset as i32)));
                    self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(X86Reg::Rax)));
                }
                reg_idx += 1;
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
        
        // Remove parameters from reg_alloc only if they were NOT register-allocated.
        // Register-allocated params were stored directly to their assigned register
        // in the prologue (callee-saved, so safe across calls). Stack-spilled params
        // need reg_alloc cleared so var_to_op returns the stack slot.
        // (Nothing to do — params without a reg_alloc entry already get stack slots.)

        for block in &func.blocks {
            // Skip unreachable blocks (marked by CFG simplification)
            if block.instructions.is_empty() && matches!(block.terminator, IrTerminator::Unreachable) {
                continue;
            }
            
            self.current_block = block.id;
            self.asm.push(X86Instr::Label(format!("{}_{}", func.name, block.id.0)));
            if self.profile_generate {
                let counter = format!("__profc_{}_{}", func.name, block.id.0);
                if let Some(counters) = self.profile_counters.as_deref_mut() {
                    if !counters.contains(&counter) {
                        counters.push(counter.clone());
                    }
                }
                self.asm.push(X86Instr::Raw(format!(
                    "    inc qword ptr {}[rip]",
                    counter
                )));
            }
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
                        // Align arrays to cache line boundaries (64 bytes) for better
                        // cache locality when the array spans multiple cache lines.
                        // Smaller allocations only need 16 bytes for SSE compatibility.
                        let alignment = if size >= 64 { 64 } else { 16 };
                        let size = (size + alignment - 1) & !(alignment - 1);
                        // Ensure next_slot is also aligned to alignment boundary
                        self.next_slot = (self.next_slot + alignment as i32 - 1) & !(alignment as i32 - 1);
                        
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
                    IrInstruction::Simd { dest, .. } => {
                        // Vector vars use XMM/YMM registers, not GPR stack slots
                        // But we need a slot for the scalar dest of HorizontalAdd
                        if let Some(d) = dest {
                            if !self.reg_alloc.contains_key(d) {
                                self.get_or_create_slot(*d);
                            }
                        }
                    }
                }
            }
        }
        
        // Parameters: allocate stack slots for parameters that were NOT
        // assigned a register. Register-allocated parameters are stored
        // directly to their callee-saved register in the prologue and
        // do not need a stack home.
        for (_, var) in &func.params {
            if !self.reg_alloc.contains_key(var) {
                self.get_or_create_slot(*var);
            }
        }
    }

    fn gen_instr(&mut self, inst: &IrInstruction) {
        match inst {
            IrInstruction::Cast { dest, src, r#type } => {
                self.gen_cast(*dest, src, r#type);
            }
            IrInstruction::Copy { dest, src} => {
                self.gen_copy(*dest, src);
            }
            IrInstruction::Binary { dest, op, left, right } => {
                let l_op = self.materialize_operand(left, X86Reg::R10);
                let r_op = self.materialize_operand(right, X86Reg::R11);

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
                self.var_types.insert(*dest, Type::ptr(r#type.clone()));
            }
            IrInstruction::Load { dest, addr, value_type, .. } => {
                gen_load(self, *dest, addr, value_type);
            }
            IrInstruction::Store { addr, src, value_type, .. } => {
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
            IrInstruction::InlineAsm { template, outputs, inputs, output_constraints, input_constraints, clobbers, is_volatile } => {
                self.gen_inline_asm(template, outputs, inputs, output_constraints, input_constraints, clobbers, *is_volatile);
            }
            IrInstruction::VaStart { list, arg_index } => {
                // va_list is a simple pointer to the next argument.
                // The register save area is at known negative offsets from RBP.
                // va_start(ap, last_fixed): ap = &save_area[arg_index + 1]
                // where arg_index is the index of the last fixed parameter.
                let next_index = *arg_index + 1;
                if let Some(save_base) = self.va_save_area_offset {
                    // Point to the next argument in the register save area
                    let offset = save_base + (next_index * 8) as i32;
                    self.asm.push(X86Instr::Lea(
                        X86Operand::Reg(X86Reg::Rax),
                        X86Operand::Mem(X86Reg::Rbp, -(offset as i32)),
                    ));
                } else {
                    // Fallback for non-variadic (shouldn't happen)
                    let offset = 16 + next_index * 8;
                    self.asm.push(X86Instr::Lea(
                        X86Operand::Reg(X86Reg::Rax),
                        X86Operand::Mem(X86Reg::Rbp, offset as i32),
                    ));
                }
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
            IrInstruction::VaArg { dest, list, r#type } => {
                // va_arg(ap, type): load value from *ap, then advance ap by 8
                // 1. Load current ap value (pointer to next arg)
                let list_op = self.operand_to_op(list);
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), list_op.clone()));
                // 2. Load the value from [rax] — always 8 bytes (all passed as 64-bit on stack)
                self.asm.push(X86Instr::Raw("mov rcx, [rax]".to_string()));
                let dest_op = self.var_to_op(*dest);
                let _ = r#type; // Type info not needed for simple pointer-based va_arg
                self.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rcx)));
                // 3. Advance ap by 8
                self.asm.push(X86Instr::Raw("add rax, 8".to_string()));
                self.asm.push(X86Instr::Mov(list_op, X86Operand::Reg(X86Reg::Rax)));
            }
            IrInstruction::Simd { op, dest, operands, elem_type, width } => {
                self.gen_simd_instruction(op, dest, operands, elem_type, *width);
            }
        }
    }

    /// Create a new temporary VarId for codegen use (e.g., struct decomposition).
    pub(crate) fn new_temp_var(&mut self) -> VarId {
        let id = self.next_temp_var;
        self.next_temp_var += 1;
        VarId(id)
    }


    pub(crate) fn get_or_create_slot(&mut self, var: VarId) -> i32 {
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
            if matches!(var_type, Type::Double) {
                let slot = self.get_or_create_slot(var);
                return X86Operand::DoubleMem(X86Reg::Rbp, slot);
            }
            if matches!(var_type, Type::Float) {
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
                let label = self.get_or_create_float_const(*f, false);
                X86Operand::RipRelLabel(label)
            }
            Operand::Var(v) => self.var_to_op(*v),
            Operand::Global(s) if s.starts_with("__label_addr_") => {
                X86Operand::GlobalQwordMem(s.clone())
            }
            Operand::Global(s) => X86Operand::Label(s.clone()),
        }
    }
    
    pub(crate) fn get_or_create_float_const(&mut self, value: f64, is_double: bool) -> String {
        let bits = value.to_bits();
        for (label, &(v, d)) in self.float_constants.iter() {
            if v.to_bits() == bits && d == is_double {
                return label.clone();
            }
        }
        let label = format!(".LC{}", self.next_float_const);
        *self.next_float_const += 1;
        self.float_constants.insert(label.clone(), (value, is_double));
        label
    }

    pub(crate) fn get_type_size(&self, r#type: &model::Type) -> usize {
        let calculator = TypeCalculator::new(self.structs, self.unions);
        calculator.get_type_size(r#type)
    }

    /// Allocate the next available XMM register for a vector variable
    fn alloc_simd_reg(&mut self, var: VarId) -> u8 {
        if let Some(&r) = self.simd_reg_map.get(&var) {
            return r;
        }
        let r = self.next_simd_reg;
        self.next_simd_reg = (self.next_simd_reg + 1).min(15);
        self.simd_reg_map.insert(var, r);
        r
    }

    /// Get the XMM register (as X86Reg) for a given index
    fn xmm_reg(index: u8) -> X86Reg {
        match index {
            0 => X86Reg::Xmm0, 1 => X86Reg::Xmm1, 2 => X86Reg::Xmm2, 3 => X86Reg::Xmm3,
            4 => X86Reg::Xmm4, 5 => X86Reg::Xmm5, 6 => X86Reg::Xmm6, 7 => X86Reg::Xmm7,
            8 => X86Reg::Xmm8, 9 => X86Reg::Xmm9, 10 => X86Reg::Xmm10, 11 => X86Reg::Xmm11,
            12 => X86Reg::Xmm12, 13 => X86Reg::Xmm13, 14 => X86Reg::Xmm14, _ => X86Reg::Xmm15,
        }
    }

    /// Get the YMM register (as X86Reg) for a given index
    fn ymm_reg(index: u8) -> X86Reg {
        match index {
            0 => X86Reg::Ymm0, 1 => X86Reg::Ymm1, 2 => X86Reg::Ymm2, 3 => X86Reg::Ymm3,
            4 => X86Reg::Ymm4, 5 => X86Reg::Ymm5, 6 => X86Reg::Ymm6, 7 => X86Reg::Ymm7,
            8 => X86Reg::Ymm8, 9 => X86Reg::Ymm9, 10 => X86Reg::Ymm10, 11 => X86Reg::Ymm11,
            12 => X86Reg::Ymm12, 13 => X86Reg::Ymm13, 14 => X86Reg::Ymm14, _ => X86Reg::Ymm15,
        }
    }

    /// Emit LEA into `dest`, routing through Rax if `dest` is a memory operand.
    fn emit_lea_to(&mut self, dest: &X86Operand, src: X86Operand) {
        match dest {
            X86Operand::Reg(_) => {
                self.asm.push(X86Instr::Lea(dest.clone(), src));
            }
            _ => {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), src));
                self.asm.push(X86Instr::Mov(dest.clone(), X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }

    /// If `operand` is an alloca buffer or global, materialize its address into `scratch_reg`
    /// and return the register operand. Otherwise return `operand_to_op(operand)`.
    pub(crate) fn materialize_operand(&mut self, operand: &Operand, scratch_reg: X86Reg) -> X86Operand {
        if let Operand::Var(var) = operand {
            if let Some(off) = self.alloca_buffers.get(var) {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(scratch_reg.clone()), X86Operand::Mem(X86Reg::Rbp, *off)));
                return X86Operand::Reg(scratch_reg);
            }
        }
        if let Operand::Global(name) = operand {
            self.asm.push(X86Instr::Lea(X86Operand::Reg(scratch_reg.clone()), X86Operand::RipRelLabel(name.clone())));
            return X86Operand::Reg(scratch_reg);
        }
        self.operand_to_op(operand)
    }

    /// Load the effective address of `operand` into `dest_reg`.
    /// Alloca buffer → LEA [rbp+off], Global → LEA name[rip], otherwise MOV from operand slot.
    pub(crate) fn load_address_into(&mut self, operand: &Operand, dest_reg: X86Reg) {
        let op = self.materialize_operand(operand, dest_reg.clone());
        if !matches!(&op, X86Operand::Reg(r) if *r == dest_reg) {
            // materialize_operand didn't load into dest_reg (normal operand), so emit MOV
            self.asm.push(X86Instr::Mov(X86Operand::Reg(dest_reg), op));
        }
    }

    /// Generate x86 instructions for an IR Cast instruction.
    fn gen_cast(&mut self, dest: VarId, src: &Operand, r#type: &Type) {
        self.var_types.insert(dest, r#type.clone());
        let d_op = self.var_to_op(dest);

        // Handle Alloca src (pointer cast)
        if let Operand::Var(var) = src {
            if let Some(off) = self.alloca_buffers.get(var) {
                let mem_op = X86Operand::Mem(X86Reg::Rbp, *off);
                self.emit_lea_to(&d_op, mem_op);
                return;
            }
        }

        let dest_is_float = matches!(r#type, Type::Float | Type::Double);
        let src_is_float = match src {
            Operand::FloatConstant(_) => true,
            Operand::Var(v) => {
                self.var_types.get(v).map(|t| matches!(t, Type::Float | Type::Double)).unwrap_or(false)
            }
            _ => false,
        };

        let s_op = self.operand_to_op(src);

        if dest_is_float && !src_is_float {
            // Int -> Float/Double
            let src_reg = if let X86Operand::Imm(_) = s_op {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op.clone()));
                X86Operand::Reg(X86Reg::Eax)
            } else {
                s_op.clone()
            };

            let dest_is_double = matches!(r#type, Type::Double);
            if dest_is_double {
                self.asm.push(X86Instr::Cvtsi2sd(X86Operand::Reg(X86Reg::Xmm0), src_reg));
                self.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                self.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), src_reg));
                self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            }
        } else if !dest_is_float && src_is_float {
            // Float/Double -> Int
            let use_rax = !matches!(d_op, X86Operand::DwordMem(..));
            let dst_reg = if use_rax { X86Reg::Rax } else { X86Reg::Eax };
            let src_is_double = match src {
                Operand::Var(v) => self.var_types.get(v).map(|t| matches!(t, Type::Double)).unwrap_or(false),
                _ => false,
            };

            if src_is_double {
                if matches!(s_op, X86Operand::DoubleMem(..) | X86Operand::Mem(..)) {
                    self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
                    self.asm.push(X86Instr::Cvttsd2si(X86Operand::Reg(dst_reg.clone()), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    self.asm.push(X86Instr::Cvttsd2si(X86Operand::Reg(dst_reg.clone()), s_op));
                }
            } else {
                if matches!(s_op, X86Operand::FloatMem(..) | X86Operand::DwordMem(..)) {
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                    self.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(dst_reg.clone()), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    self.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(dst_reg.clone()), s_op));
                }
            }
            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(dst_reg)));
        } else if dest_is_float && src_is_float {
            // Float<->Double conversion or same-type copy
            let dest_is_double = matches!(r#type, Type::Double);
            let src_is_double = match src {
                Operand::Var(v) => self.var_types.get(v).map(|t| matches!(t, Type::Double)).unwrap_or(false),
                _ => false,
            };
            if !src_is_double && dest_is_double {
                // float -> double
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                self.asm.push(X86Instr::Cvtss2sd(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
                self.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else if src_is_double && !dest_is_double {
                // double -> float
                self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
                self.asm.push(X86Instr::Cvtsd2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
                self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else if dest_is_double {
                self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
                self.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
            }
        } else {
            // Int-to-int: check operand sizes to avoid invalid mov instructions
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

    /// Generate x86 instructions for an IR Copy instruction.
    fn gen_copy(&mut self, dest: VarId, src: &Operand) {
        if !self.var_types.contains_key(&dest) {
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
                self.var_types.insert(dest, t);
            }
        }

        let s_op = self.operand_to_op(src);
        let d_op = self.var_to_op(dest);

        // Handle Global variables (load address)
        if let X86Operand::Label(name) = &s_op {
            let rip_rel = X86Operand::RipRelLabel(name.clone());
            self.emit_lea_to(&d_op, rip_rel);
            return;
        }

        // Handle Alloca addresses (arrays/structs on stack) -> LEA
        if let Operand::Var(var) = src {
            if let Some(off) = self.alloca_buffers.get(var) {
                let mem_op = X86Operand::Mem(X86Reg::Rbp, *off);
                self.emit_lea_to(&d_op, mem_op);
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

        // Check if float/double
        if matches!(s_op, X86Operand::DoubleMem(..)) || matches!(d_op, X86Operand::DoubleMem(..)) {
            self.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
            self.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
        } else if matches!(s_op, X86Operand::FloatMem(..)) || matches!(d_op, X86Operand::FloatMem(..)) {
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
                // 32-bit source to non-32-bit dest (64-bit mem or reg)
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), s_op));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            } else if !src_is_dword && dst_is_dword {
                // 64-bit source to 32-bit dest
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Eax)));
            } else {
                // Writing eax already zero-extends into rax on x86-64.
                if matches!(d_op, X86Operand::Reg(X86Reg::Rax))
                    && matches!(s_op, X86Operand::Reg(X86Reg::Eax))
                {
                    return;
                }
                self.asm.push(X86Instr::Mov(d_op, s_op));
            }
        }
    }

    /// Scalar fallback for gather when AVX2 gather is unavailable.
    fn emit_scalar_gather(&mut self, dest_idx: u8, index_idx: u8, width: usize) {
        let chunks = (width + 3) / 4;
        for chunk in 0..chunks {
            let lanes = (width - chunk * 4).min(4);
            let (idx_part, dest_part) = if chunk == 0 && width <= 4 {
                (
                    Self::xmm_reg(index_idx).to_str().to_string(),
                    Self::xmm_reg(dest_idx).to_str().to_string(),
                )
            } else if chunk == 0 {
                (
                    Self::xmm_reg(index_idx).to_str().to_string(),
                    Self::xmm_reg(dest_idx).to_str().to_string(),
                )
            } else {
                let ymm_idx = Self::ymm_reg(index_idx).to_str().to_string();
                let ymm_dest = Self::ymm_reg(dest_idx).to_str().to_string();
                self.asm.push(X86Instr::Raw(format!(
                    "  vextracti128 xmm15, {}, 1",
                    ymm_idx
                )));
                self.asm.push(X86Instr::Raw(format!(
                    "  vextracti128 xmm14, {}, 1",
                    ymm_dest
                )));
                ("xmm15".to_string(), "xmm14".to_string())
            };
            for lane in 0..lanes {
                self.asm.push(X86Instr::Raw(format!(
                    "  pextrd eax, {}, {}",
                    idx_part, lane
                )));
                self.asm.push(X86Instr::Raw("  mov ecx, [r10 + rax*4]".to_string()));
                self.asm.push(X86Instr::Raw(format!(
                    "  pinsrd {}, ecx, {}",
                    dest_part, lane
                )));
            }
            if chunk == 1 && width > 4 {
                let ymm_dest = Self::ymm_reg(dest_idx).to_str().to_string();
                self.asm.push(X86Instr::Raw(format!(
                    "  vinserti128 {}, {}, xmm14, 1",
                    ymm_dest, ymm_dest
                )));
            }
        }
    }

    /// Scalar fallback for scatter when AVX2 scatter is unavailable.
    fn emit_scalar_scatter(&mut self, index_idx: u8, value_idx: u8, width: usize) {
        let chunks = (width + 3) / 4;
        for chunk in 0..chunks {
            let lanes = (width - chunk * 4).min(4);
            let (idx_part, val_part) = if chunk == 0 && width <= 4 {
                (
                    Self::xmm_reg(index_idx).to_str().to_string(),
                    Self::xmm_reg(value_idx).to_str().to_string(),
                )
            } else if chunk == 0 {
                (
                    Self::xmm_reg(index_idx).to_str().to_string(),
                    Self::xmm_reg(value_idx).to_str().to_string(),
                )
            } else {
                let ymm_idx = Self::ymm_reg(index_idx).to_str().to_string();
                let ymm_val = Self::ymm_reg(value_idx).to_str().to_string();
                self.asm.push(X86Instr::Raw(format!(
                    "  vextracti128 xmm15, {}, 1",
                    ymm_idx
                )));
                self.asm.push(X86Instr::Raw(format!(
                    "  vextracti128 xmm14, {}, 1",
                    ymm_val
                )));
                ("xmm15".to_string(), "xmm14".to_string())
            };
            for lane in 0..lanes {
                self.asm.push(X86Instr::Raw(format!(
                    "  pextrd eax, {}, {}",
                    idx_part, lane
                )));
                self.asm.push(X86Instr::Raw(format!(
                    "  pextrd ecx, {}, {}",
                    val_part, lane
                )));
                self.asm.push(X86Instr::Raw("  mov [r10 + rax*4], ecx".to_string()));
            }
        }
    }

    fn emit_all_ones_mask(&mut self, width: usize, use_avx: bool) {
        let spill = if width > 4 { 32 } else { 16 };
        self.asm.push(X86Instr::Raw(format!("  sub rsp, {}", spill)));
        for lane in 0..width {
            self.asm.push(X86Instr::Mov(
                X86Operand::DwordMem(X86Reg::Rsp, (lane * 4) as i32),
                X86Operand::Imm(-1),
            ));
        }
        if use_avx {
            self.asm.push(X86Instr::Vmovdqu(
                X86Operand::Reg(Self::ymm_reg(15)),
                X86Operand::YmmwordMem(X86Reg::Rsp, 0),
            ));
        } else {
            self.asm.push(X86Instr::Movdqu(
                X86Operand::Reg(X86Reg::Xmm15),
                X86Operand::XmmwordMem(X86Reg::Rsp, 0),
            ));
        }
        self.asm.push(X86Instr::Raw(format!("  add rsp, {}", spill)));
    }

    /// Generate x86 SIMD instructions for an IR Simd instruction
    fn gen_simd_instruction(
        &mut self,
        op: &SimdOp,
        dest: &Option<VarId>,
        operands: &[Operand],
        elem_type: &Type,
        width: usize,
    ) {
        let is_float = matches!(elem_type, Type::Float | Type::Double);
        // 256-bit ymm ops (especially AVX2 integer ops like vpandd/vpmulld) need AVX2.
        let use_avx = width > 4
            && self.target.simd_level >= model::SimdLevel::AVX2;

        match op {
            SimdOp::Load => {
                // operands[0] = address (Var holding pointer)
                let dest_var = dest.expect("VectorLoad must have dest");
                let reg_idx = self.alloc_simd_reg(dest_var);

                // Load address into R10
                let addr_op = self.operand_to_op(&operands[0]);
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), addr_op));

                if use_avx {
                    let ymm = X86Operand::Reg(Self::ymm_reg(reg_idx));
                    let mem = X86Operand::YmmwordMem(X86Reg::R10, 0);
                    if is_float {
                        self.asm.push(X86Instr::Vmovups(ymm, mem));
                    } else {
                        self.asm.push(X86Instr::Vmovdqu(ymm, mem));
                    }
                } else {
                    let xmm = X86Operand::Reg(Self::xmm_reg(reg_idx));
                    let mem = X86Operand::XmmwordMem(X86Reg::R10, 0);
                    if is_float {
                        self.asm.push(X86Instr::Movups(xmm, mem));
                    } else {
                        self.asm.push(X86Instr::Movdqu(xmm, mem));
                    }
                }
            }

            SimdOp::Store => {
                // operands[0] = address, operands[1] = Var(source vector)
                let src_var = match &operands[1] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let src_idx = self.simd_reg_map.get(&src_var).copied().unwrap_or(0);

                // Load address into R10
                let addr_op = self.operand_to_op(&operands[0]);
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), addr_op));

                if use_avx {
                    let ymm = X86Operand::Reg(Self::ymm_reg(src_idx));
                    let mem = X86Operand::YmmwordMem(X86Reg::R10, 0);
                    if is_float {
                        self.asm.push(X86Instr::Vmovups(mem, ymm));
                    } else {
                        self.asm.push(X86Instr::Vmovdqu(mem, ymm));
                    }
                } else {
                    let xmm = X86Operand::Reg(Self::xmm_reg(src_idx));
                    let mem = X86Operand::XmmwordMem(X86Reg::R10, 0);
                    if is_float {
                        self.asm.push(X86Instr::Movups(mem, xmm));
                    } else {
                        self.asm.push(X86Instr::Movdqu(mem, xmm));
                    }
                }
            }

            SimdOp::Add | SimdOp::Sub | SimdOp::Mul | SimdOp::And | SimdOp::Or | SimdOp::Xor => {
                // operands[0] = Var(left), operands[1] = Var(right)
                let dest_var = dest.expect("VectorBinary must have dest");
                let left_var = match &operands[0] { Operand::Var(v) => *v, _ => return };
                let right_var = match &operands[1] { Operand::Var(v) => *v, _ => return };

                let left_idx = self.simd_reg_map.get(&left_var).copied().unwrap_or(0);
                let right_idx = self.simd_reg_map.get(&right_var).copied().unwrap_or(0);
                let dest_idx = self.alloc_simd_reg(dest_var);

                if use_avx {
                    let dst = X86Operand::Reg(Self::ymm_reg(dest_idx));
                    let s1 = X86Operand::Reg(Self::ymm_reg(left_idx));
                    let s2 = X86Operand::Reg(Self::ymm_reg(right_idx));
                    if is_float {
                        match op {
                            SimdOp::Add => self.asm.push(X86Instr::Vaddps(dst, s1, s2)),
                            SimdOp::Sub => self.asm.push(X86Instr::Vsubps(dst, s1, s2)),
                            SimdOp::Mul => self.asm.push(X86Instr::Vmulps(dst, s1, s2)),
                            _ => return,
                        }
                    } else {
                        match op {
                            SimdOp::Add => self.asm.push(X86Instr::Vpaddd(dst, s1, s2)),
                            SimdOp::Sub => self.asm.push(X86Instr::Vpsubd(dst, s1, s2)),
                            SimdOp::Mul => self.asm.push(X86Instr::Vpmulld(dst, s1, s2)),
                            SimdOp::And => {
                                if is_float {
                                    self.asm.push(X86Instr::Vandps(dst, s1, s2));
                                } else {
                                    self.asm.push(X86Instr::Vandps(dst, s1, s2));
                                }
                            }
                            SimdOp::Or => self.asm.push(X86Instr::Vpord(dst, s1, s2)),
                            SimdOp::Xor => self.asm.push(X86Instr::Vpxor(dst, s1, s2)),
                            _ => return,
                        }
                    }
                } else {
                    // SSE: 2-operand form, dest = left op right
                    // Copy left to dest first if needed
                    let dst_xmm = X86Operand::Reg(Self::xmm_reg(dest_idx));
                    let left_xmm = X86Operand::Reg(Self::xmm_reg(left_idx));
                    let right_xmm = X86Operand::Reg(Self::xmm_reg(right_idx));

                    if dest_idx != left_idx {
                        if is_float {
                            self.asm.push(X86Instr::Movaps(dst_xmm.clone(), left_xmm));
                        } else {
                            self.asm.push(X86Instr::Movdqa(dst_xmm.clone(), left_xmm));
                        }
                    }

                    if is_float {
                        match op {
                            SimdOp::Add => self.asm.push(X86Instr::Addps(dst_xmm, right_xmm)),
                            SimdOp::Sub => self.asm.push(X86Instr::Subps(dst_xmm, right_xmm)),
                            SimdOp::Mul => self.asm.push(X86Instr::Mulps(dst_xmm, right_xmm)),
                            _ => return,
                        }
                    } else {
                        match op {
                            SimdOp::Add => self.asm.push(X86Instr::Paddd(dst_xmm, right_xmm)),
                            SimdOp::Sub => self.asm.push(X86Instr::Psubd(dst_xmm, right_xmm)),
                            SimdOp::Mul => self.asm.push(X86Instr::Pmulld(dst_xmm, right_xmm)),
                            SimdOp::And => self.asm.push(X86Instr::Pand(dst_xmm, right_xmm)),
                            SimdOp::Or => self.asm.push(X86Instr::Por(dst_xmm, right_xmm)),
                            SimdOp::Xor => self.asm.push(X86Instr::Pxor(dst_xmm, right_xmm)),
                            _ => return,
                        }
                    }
                }
            }

            SimdOp::Splat => {
                // operands[0] = scalar value to broadcast
                let dest_var = dest.expect("Splat must have dest");
                let dest_idx = self.alloc_simd_reg(dest_var);

                // Move scalar into eax first (32-bit for i32 elements)
                let scalar_op = self.operand_to_op(&operands[0]);
                // Convert operand to 32-bit for eax destination
                let scalar_op_32 = match scalar_op {
                    X86Operand::Mem(reg, offset) => X86Operand::DwordMem(reg, offset),
                    X86Operand::Reg(r) => X86Operand::Reg(r.to_32bit()),
                    other => other,
                };
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), scalar_op_32));

                if use_avx {
                    // AVX2: movd xmm_dst, eax; vpbroadcastd ymm_dst, xmm_dst
                    let xmm_dst = X86Operand::Reg(Self::xmm_reg(dest_idx));
                    let ymm_dst = X86Operand::Reg(Self::ymm_reg(dest_idx));
                    self.asm.push(X86Instr::Movd(xmm_dst.clone(), X86Operand::Reg(X86Reg::Eax)));
                    self.asm.push(X86Instr::Vpbroadcastd(ymm_dst, xmm_dst));
                } else {
                    // SSE: movd xmm_dst, eax; pshufd xmm_dst, xmm_dst, 0x00
                    let xmm_dst = X86Operand::Reg(Self::xmm_reg(dest_idx));
                    self.asm.push(X86Instr::Movd(xmm_dst.clone(), X86Operand::Reg(X86Reg::Eax)));
                    self.asm.push(X86Instr::Pshufd(xmm_dst.clone(), xmm_dst, 0x00));
                }
            }

            SimdOp::LaneMask => {
                // operands[0] = scalar IV, operands[1] = bound (scalar)
                // Build mask in a stack slot: lane k is all-1s iff (iv + k) < bound.
                let dest_var = dest.expect("LaneMask must have dest");
                let dest_idx = self.alloc_simd_reg(dest_var);
                let iv_op = self.operand_to_op(&operands[0]);
                let bound_op = self.operand_to_op(&operands[1]);
                let iv32 = match &iv_op {
                    X86Operand::Reg(r) => X86Operand::Reg(r.to_32bit()),
                    X86Operand::Mem(r, off) => X86Operand::DwordMem(r.clone(), *off),
                    other => other.clone(),
                };
                let bound32 = match &bound_op {
                    X86Operand::Reg(r) => X86Operand::Reg(r.to_32bit()),
                    X86Operand::Mem(r, off) => X86Operand::DwordMem(r.clone(), *off),
                    X86Operand::Imm(v) => X86Operand::Imm(*v),
                    other => other.clone(),
                };

                let spill = if width > 4 { 32 } else { 16 };
                self.asm.push(X86Instr::Raw(format!("  sub rsp, {}", spill)));
                for lane in 0..width {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), iv32.clone()));
                    if lane != 0 {
                        self.asm.push(X86Instr::Add(
                            X86Operand::Reg(X86Reg::Eax),
                            X86Operand::Imm(lane as i64),
                        ));
                    }
                    self.asm.push(X86Instr::Cmp(
                        X86Operand::Reg(X86Reg::Eax),
                        bound32.clone(),
                    ));
                    self.asm.push(X86Instr::Set("l".to_string(), X86Operand::Reg(X86Reg::Al)));
                    self.asm.push(X86Instr::Movzx(
                        X86Operand::Reg(X86Reg::Eax),
                        X86Operand::Reg(X86Reg::Al),
                    ));
                    self.asm.push(X86Instr::Neg(X86Operand::Reg(X86Reg::Eax)));
                    self.asm.push(X86Instr::Mov(
                        X86Operand::DwordMem(X86Reg::Rsp, (lane * 4) as i32),
                        X86Operand::Reg(X86Reg::Eax),
                    ));
                }
                if use_avx {
                    self.asm.push(X86Instr::Vmovdqu(
                        X86Operand::Reg(Self::ymm_reg(dest_idx)),
                        X86Operand::YmmwordMem(X86Reg::Rsp, 0),
                    ));
                } else {
                    self.asm.push(X86Instr::Movdqu(
                        X86Operand::Reg(Self::xmm_reg(dest_idx)),
                        X86Operand::XmmwordMem(X86Reg::Rsp, 0),
                    ));
                }
                self.asm.push(X86Instr::Raw(format!("  add rsp, {}", spill)));
            }

            SimdOp::Blend => {
                // operands[0] = old mem, [1] = new value, [2] = mask
                let dest_var = dest.expect("Blend must have dest");
                let old_var = match &operands[0] { Operand::Var(v) => *v, _ => return };
                let new_var = match &operands[1] { Operand::Var(v) => *v, _ => return };
                let mask_var = match &operands[2] { Operand::Var(v) => *v, _ => return };
                let dest_idx = self.alloc_simd_reg(dest_var);
                let old_idx = self.simd_reg_map.get(&old_var).copied().unwrap_or(0);
                let new_idx = self.simd_reg_map.get(&new_var).copied().unwrap_or(0);
                let mask_idx = self.simd_reg_map.get(&mask_var).copied().unwrap_or(0);

                if use_avx {
                    let dst = X86Operand::Reg(Self::ymm_reg(dest_idx));
                    let old = X86Operand::Reg(Self::ymm_reg(old_idx));
                    let new = X86Operand::Reg(Self::ymm_reg(new_idx));
                    let mask = X86Operand::Reg(Self::ymm_reg(mask_idx));
                    let tmp = X86Operand::Reg(Self::ymm_reg(15));
                    // dst = (~mask & old) | (new & mask) via AVX1 float bitwise ops
                    self.asm.push(X86Instr::Vandnps(tmp.clone(), mask.clone(), old));
                    self.asm.push(X86Instr::Vandps(dst.clone(), new.clone(), mask.clone()));
                    self.asm.push(X86Instr::Vorps(dst.clone(), tmp, dst));
                } else {
                    let dst = X86Operand::Reg(Self::xmm_reg(dest_idx));
                    let old = X86Operand::Reg(Self::xmm_reg(old_idx));
                    let new = X86Operand::Reg(Self::xmm_reg(new_idx));
                    let mask = X86Operand::Reg(Self::xmm_reg(mask_idx));
                    let tmp = X86Operand::Reg(X86Reg::Xmm15);
                    // tmp = (~mask) & old; dst = (new & mask) | tmp
                    self.asm.push(X86Instr::Movaps(tmp.clone(), mask.clone()));
                    self.asm.push(X86Instr::Pandn(tmp.clone(), old));
                    self.asm.push(X86Instr::Movaps(dst.clone(), new));
                    self.asm.push(X86Instr::Pand(dst.clone(), mask));
                    self.asm.push(X86Instr::Por(dst, tmp));
                }
            }

            SimdOp::HorizontalAdd => {
                // operands[0] = Var(vector to reduce)
                // dest = scalar VarId to receive the sum
                let dest_var = dest.expect("HorizontalAdd must have dest");
                let src_var = match &operands[0] { Operand::Var(v) => *v, _ => return };
                let src_idx = self.simd_reg_map.get(&src_var).copied().unwrap_or(0);

                if use_avx {
                    // AVX2: ymm → xmm reduction
                    // Step 1: extract high 128 bits and add to low 128 bits
                    let ymm_src = X86Operand::Reg(Self::ymm_reg(src_idx));
                    let xmm_src = X86Operand::Reg(Self::xmm_reg(src_idx));
                    let xmm_tmp = X86Operand::Reg(X86Reg::Xmm15);
                    self.asm.push(X86Instr::Vextracti128(xmm_tmp.clone(), ymm_src, 1));
                    if is_float {
                        self.asm.push(X86Instr::Addps(xmm_src.clone(), xmm_tmp));
                    } else {
                        self.asm.push(X86Instr::Paddd(xmm_src.clone(), xmm_tmp));
                    }
                    // Now reduce the 128-bit xmm_src
                }

                // Reduce 128-bit XMM register to scalar
                let xmm_src = X86Operand::Reg(Self::xmm_reg(src_idx));
                let xmm_tmp = X86Operand::Reg(X86Reg::Xmm15);

                if is_float {
                    // [a,b,c,d] → pshufd tmp, src, 0x4E (swap halves) → addps → pshufd → addps → movd
                    self.asm.push(X86Instr::Pshufd(xmm_tmp.clone(), xmm_src.clone(), 0x4E));
                    self.asm.push(X86Instr::Addps(xmm_src.clone(), xmm_tmp.clone()));
                    self.asm.push(X86Instr::Pshufd(xmm_tmp.clone(), xmm_src.clone(), 0xB1));
                    self.asm.push(X86Instr::Addps(xmm_src.clone(), xmm_tmp));
                    // Move scalar float to dest
                    let d_op = self.var_to_op(dest_var);
                    self.asm.push(X86Instr::Movss(d_op, xmm_src));
                } else {
                    // [a,b,c,d] → pshufd tmp, src, 0x4E → paddd → pshufd → paddd → movd
                    self.asm.push(X86Instr::Pshufd(xmm_tmp.clone(), xmm_src.clone(), 0x4E));
                    self.asm.push(X86Instr::Paddd(xmm_src.clone(), xmm_tmp.clone()));
                    self.asm.push(X86Instr::Pshufd(xmm_tmp.clone(), xmm_src.clone(), 0xB1));
                    self.asm.push(X86Instr::Paddd(xmm_src.clone(), xmm_tmp));
                    // movd eax, xmm_src; movsxd rax, eax; mov dest, rax
                    self.asm.push(X86Instr::Movd(X86Operand::Reg(X86Reg::Eax), xmm_src));
                    self.asm.push(X86Instr::Raw("  cdqe".to_string()));
                    let d_op = self.var_to_op(dest_var);
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }

                if use_avx {
                    self.asm.push(X86Instr::Vzeroupper);
                }
            }

            SimdOp::IndexSeq => {
                let dest_var = dest.expect("IndexSeq must have dest");
                let dest_idx = self.alloc_simd_reg(dest_var);
                let iv_op = self.operand_to_op(&operands[0]);
                let scale = match &operands[1] {
                    Operand::Constant(s) => *s,
                    _ => 1,
                };
                let offset = match &operands[2] {
                    Operand::Constant(o) => *o,
                    _ => 0,
                };
                let iv32 = match iv_op {
                    X86Operand::Reg(r) => X86Operand::Reg(r.to_32bit()),
                    X86Operand::Mem(r, off) => X86Operand::DwordMem(r.clone(), off),
                    other => other,
                };
                let spill = if width > 4 { 32 } else { 16 };
                self.asm.push(X86Instr::Raw(format!("  sub rsp, {}", spill)));
                for lane in 0..width {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), iv32.clone()));
                    if lane != 0 {
                        self.asm.push(X86Instr::Add(
                            X86Operand::Reg(X86Reg::Eax),
                            X86Operand::Imm(lane as i64),
                        ));
                    }
                    if scale != 1 {
                        self.asm.push(X86Instr::Imul(
                            X86Operand::Reg(X86Reg::Eax),
                            X86Operand::Imm(scale),
                        ));
                    }
                    if offset != 0 {
                        self.asm.push(X86Instr::Add(
                            X86Operand::Reg(X86Reg::Eax),
                            X86Operand::Imm(offset),
                        ));
                    }
                    self.asm.push(X86Instr::Mov(
                        X86Operand::DwordMem(X86Reg::Rsp, (lane * 4) as i32),
                        X86Operand::Reg(X86Reg::Eax),
                    ));
                }
                if use_avx {
                    self.asm.push(X86Instr::Vmovdqu(
                        X86Operand::Reg(Self::ymm_reg(dest_idx)),
                        X86Operand::YmmwordMem(X86Reg::Rsp, 0),
                    ));
                } else {
                    self.asm.push(X86Instr::Movdqu(
                        X86Operand::Reg(Self::xmm_reg(dest_idx)),
                        X86Operand::XmmwordMem(X86Reg::Rsp, 0),
                    ));
                }
                self.asm.push(X86Instr::Raw(format!("  add rsp, {}", spill)));
            }

            SimdOp::Gather => {
                let dest_var = dest.expect("Gather must have dest");
                let dest_idx = self.alloc_simd_reg(dest_var);
                let base_var = match &operands[0] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let index_var = match &operands[1] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let index_idx = self.simd_reg_map.get(&index_var).copied().unwrap_or(0);
                self.load_address_into(
                    &Operand::Var(base_var),
                    X86Reg::R10,
                );

                if use_avx && self.target.simd_level >= model::SimdLevel::AVX2 {
                    self.emit_all_ones_mask(width, true);
                    let dest = X86Operand::Reg(Self::ymm_reg(dest_idx));
                    let idx = X86Operand::Reg(Self::ymm_reg(index_idx));
                    let mask = X86Operand::Reg(Self::ymm_reg(15));
                    self.asm.push(X86Instr::Vpgatherdd(dest, idx, mask));
                } else {
                    self.emit_scalar_gather(dest_idx, index_idx, width);
                }
            }

            SimdOp::Scatter => {
                let base_var = match &operands[0] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let index_var = match &operands[1] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let value_var = match &operands[2] {
                    Operand::Var(v) => *v,
                    _ => return,
                };
                let index_idx = self.simd_reg_map.get(&index_var).copied().unwrap_or(0);
                let value_idx = self.simd_reg_map.get(&value_var).copied().unwrap_or(0);
                self.load_address_into(
                    &Operand::Var(base_var),
                    X86Reg::R10,
                );

                // Binutils GAS does not accept AVX2 vpscatterdd in Intel syntax; use scalar stores.
                let _ = use_avx;
                self.emit_scalar_scatter(index_idx, value_idx, width);
            }
        }
    }
}
