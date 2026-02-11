mod x86;
mod regalloc;
mod peephole;
mod types;
mod instructions;

use model::{BinaryOp, UnaryOp, Type};
use ir::{IRProgram, Function as IrFunction, BlockId, VarId, Operand, Instruction as IrInstruction, Terminator as IrTerminator};
use std::collections::HashMap;

pub use x86::{X86Reg, X86Operand, X86Instr, emit_asm};
pub use regalloc::{PhysicalReg, allocate_registers};
use peephole::apply_peephole;
use types::TypeCalculator;
use instructions::InstructionGenerator;

pub struct Codegen {
    // SSA Var -> Stack Offset (for spills or non-register vars)
    stack_slots: HashMap<VarId, i32>,
    next_slot: i32,
    asm: Vec<X86Instr>,
    structs: HashMap<String, model::StructDef>,
    unions: HashMap<String, model::UnionDef>,
    // Register allocation
    reg_alloc: HashMap<VarId, PhysicalReg>,
    enable_regalloc: bool,
    // Float constants
    float_constants: HashMap<String, f64>, // label -> value
    next_float_const: usize,
    // Variable types (for float vs int differentiation)
    var_types: HashMap<VarId, Type>,
    // Function signatures (for call return type inference)
    func_return_types: HashMap<String, Type>,
    // Alloca variables -> direct stack buffer offset (for direct memory access)
    alloca_buffers: HashMap<VarId, i32>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            stack_slots: HashMap::new(),
            next_slot: 0,
            asm: Vec::new(),
            structs: HashMap::new(),
            unions: HashMap::new(),
            reg_alloc: HashMap::new(),
            enable_regalloc: true, // Enable register allocation by default
            float_constants: HashMap::new(),
            next_float_const: 0,
            var_types: HashMap::new(),
            func_return_types: HashMap::new(),
            alloca_buffers: HashMap::new(),
        }
    }

    pub fn gen_program(&mut self, prog: &IRProgram) -> String {
        self.structs.clear();
        self.unions.clear();
        for s_def in &prog.structs {
            self.structs.insert(s_def.name.clone(), s_def.clone());
        }
        for u_def in &prog.unions {
            self.unions.insert(u_def.name.clone(), u_def.clone());
        }
        self.float_constants.clear();
        self.next_float_const = 0;
        
        // Build function signature map for return type inference in calls
        self.func_return_types.clear();
        for func in &prog.functions {
            self.func_return_types.insert(func.name.clone(), func.return_type.clone());
        }
        
        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n");
        
        // Strings
        if !prog.global_strings.is_empty() {
            output.push_str(".data\n");
            for (label, content) in &prog.global_strings {
                // Properly escape string for assembly output
                let escaped = content
                    .replace("\\", "\\\\")  // Backslash must be first
                    .replace("\n", "\\n")   // Newline
                    .replace("\r", "\\r")   // Carriage return
                    .replace("\t", "\\t")   // Tab
                    .replace("\"", "\\\"")  // Double quote
                    .replace("\0", "\\0");  // Null (though .asciz adds one)
                output.push_str(&format!("{}: .asciz \"{}\"\n", label, escaped));
            }
        }

        // Globals (use .quad for all to simplify codegen - int values will be sign-extended)
        if !prog.globals.is_empty() {
             if prog.global_strings.is_empty() { output.push_str(".data\n"); }
             for g in &prog.globals {
                 // Handle section attribute
                 let mut in_custom_section = false;
                 for attr in &g.attributes {
                     if let model::Attribute::Section(section_name) = attr {
                         output.push_str(&format!(".section {}\n", section_name));
                         in_custom_section = true;
                         break;
                     }
                 }
                 
                 output.push_str(&format!(".globl {}\n", g.name));
                 
                 // Handle aligned attribute (default to 4-byte alignment otherwise)
                 let mut alignment = 4;
                 for attr in &g.attributes {
                     if let model::Attribute::Aligned(n) = attr {
                         alignment = *n;
                         break;
                     }
                 }
                 output.push_str(&format!(".align {}\n", alignment));
                 
                 if let Some(init) = &g.init {
                     // Initialized - use .quad for all for simplicity
                     match &g.r#type {
                        Type::Array(_, _size) => {
                             let bytes = self.get_type_size(&g.r#type);
                             output.push_str(&format!("{}: .zero {}\n", g.name, bytes));
                        }
                        _ => {
                            if let model::Expr::Constant(c) = init {
                                output.push_str(&format!("{}: .long {}\n", g.name, c));
                            } else {
                                output.push_str(&format!("{}: .long 0\n", g.name));
                            }
                        }
                     }
                 } else {
                     // Uninitialized
                     output.push_str(&format!("{}: .long 0\n", g.name));
                 }
                 
                 // Switch back to .data section if we were in a custom section
                 if in_custom_section {
                     output.push_str(".data\n");
                 }
             }
        }

        output.push_str(".text\n");
        output.push_str(".globl main\n\n");
        
        for func in &prog.functions {
            // Emit .globl directive for all functions (for linking)
            output.push_str(&format!(".globl {}\n", func.name));
            
            self.gen_function(func);
            
            // Apply peephole optimizations
            apply_peephole(&mut self.asm);
            
            output.push_str(&emit_asm(&self.asm));
            self.asm.clear();
            self.stack_slots.clear();
            self.reg_alloc.clear();
            self.alloca_buffers.clear();
            self.next_slot = 0;
        }
        
        // Emit float constants in .data section
        if !self.float_constants.is_empty() {
            output.push_str("\n.data\n");
            output.push_str(".align 16\n");
            let mut sorted_consts: Vec<_> = self.float_constants.iter().collect();
            sorted_consts.sort_by_key(|(label, _)| label.as_str());
            for (label, value) in sorted_consts {
                // Convert f64 to f32 for single-precision floats
                let f32_value = *value as f32;
                let bits = f32_value.to_bits();
                output.push_str(&format!("{}: .long 0x{:08x}\n", label, bits));
            }
        }
        
        output
    }

    fn gen_function(&mut self, func: &IrFunction) {
        // Perform register allocation
        if self.enable_regalloc {
            self.reg_alloc = allocate_registers(func);
        }
        
        self.asm.push(X86Instr::Label(func.name.clone()));
        
        // Prologue
        self.asm.push(X86Instr::Push(X86Reg::Rbp));
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rbp), X86Operand::Reg(X86Reg::Rsp)));
        
        self.allocate_stack_slots(func);
        let locals_size = self.next_slot;
        let shadow_space = 32;
        let total_stack = (locals_size + shadow_space + 15) & !15;
        
        if total_stack > 0 {
            self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rsp), X86Operand::Imm(total_stack as i64)));
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
    }

    fn allocate_stack_slots(&mut self, func: &IrFunction) {
        // Only allocate stack slots for variables that:
        // 1. Need Alloca (arrays/structs) - these always need stack space
        // 2. Didn't get a register assigned (spilled variables)
        
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    IrInstruction::Alloca { dest, r#type } => {
                        // Allocate buffer space directly on stack (aligned to 8 bytes)
                        let size = self.get_type_size(r#type) as i32;
                        let aligned_size = ((size + 7) / 8) * 8;
                        self.next_slot += aligned_size;
                        let buffer_offset = -self.next_slot;
                        // Track this as an alloca for direct memory access
                        self.alloca_buffers.insert(*dest, buffer_offset);
                    }
                    IrInstruction::Binary { dest, .. } |
                    IrInstruction::FloatBinary { dest, .. } |
                    IrInstruction::Unary { dest, .. } |
                    IrInstruction::FloatUnary { dest, .. } |
                    IrInstruction::Phi { dest, .. } |
                    IrInstruction::Copy { dest, .. } |
                    IrInstruction::Load { dest, .. } |
                    IrInstruction::GetElementPtr { dest, .. } => {
                        // Only allocate if no register was assigned
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
                        // Allocate stack slots for outputs without registers
                        for var in outputs {
                            if !self.reg_alloc.contains_key(var) {
                                self.get_or_create_slot(*var);
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

    fn get_or_create_slot(&mut self, var: VarId) -> i32 {
        if let Some(slot) = self.stack_slots.get(&var) {
            return *slot;
        }
        self.next_slot += 8;
        let slot = -self.next_slot;
        self.stack_slots.insert(var, slot);
        slot
    }

    fn var_to_op(&mut self, var: VarId) -> X86Operand {
        // Float variables must use stack slots (no XMM register allocation yet)
        if let Some(var_type) = self.var_types.get(&var) {
            if matches!(var_type, Type::Float | Type::Double) {
                let slot = self.get_or_create_slot(var);
                return X86Operand::FloatMem(X86Reg::Rbp, slot);
            }
        }
        
        // Check if variable has a register allocated
        if let Some(reg) = self.reg_alloc.get(&var) {
            return X86Operand::Reg(reg.to_x86());
        }
        
        // Fall back to stack slot
        let slot = self.get_or_create_slot(var);
        X86Operand::Mem(X86Reg::Rbp, slot)
    }

    fn operand_to_op(&mut self, op: &Operand) -> X86Operand {
        match op {
            Operand::Constant(c) => X86Operand::Imm(*c),
            Operand::FloatConstant(f) => {
                // Return RIP-relative label for the float constant
                let label = self.get_or_create_float_const(*f);
                X86Operand::RipRelLabel(label)
            }
            Operand::Var(v) => self.var_to_op(*v),
            Operand::Global(s) => X86Operand::Label(s.clone()),
        }
    }
    
    fn get_or_create_float_const(&mut self, value: f64) -> String {
        // Check if we already have this constant
        for (label, &v) in &self.float_constants {
            if (v - value).abs() < f64::EPSILON {
                return label.clone();
            }
        }
        // Create new constant
        let label = format!(".LC{}", self.next_float_const);
        self.next_float_const += 1;
        self.float_constants.insert(label.clone(), value);
        label
    }

    fn gen_instr(&mut self, inst: &IrInstruction) {
        match inst {
            IrInstruction::Copy { dest, src} => {
                let d_op = self.var_to_op(*dest);
                // Special handling for Global sources (function addresses, globals)
                if let Operand::Global(name) = src {
                    // Use LEA with RIP-relative addressing to get the address
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    return;
                }
                
                // Special handling for alloca sources (array decay to pointer)
                if let Operand::Var(src_var) = src {
                    if let Some(buffer_offset) = self.alloca_buffers.get(src_var) {
                        // This is an alloca (array) - get its address with LEA
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                        return;
                    }
                }
                
                // Special handling for FloatConstant - load with movss into xmm0 then store to stack
                if let Operand::FloatConstant(_) = src {
                    let label = self.operand_to_op(src);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                    self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                    return;
                }
                
                let s_op = self.operand_to_op(src);
                
                // ADDITIONAL CHECK: If s_op is a Mem operand that matches an alloca buffer offset,
                // use LEA to get the address instead of MOV to load the value
                if let X86Operand::Mem(X86Reg::Rbp, offset) = s_op {
                    // Check if this offset corresponds to any alloca buffer
                    let is_alloca_buffer = self.alloca_buffers.values().any(|&buf_offset| buf_offset == offset);
                    if is_alloca_buffer {
                        // This is referencing an alloca buffer - use LEA to get address
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                        return;
                    }
                }
                
                // Check if source is memory or if we need intermediate register
                // x86 doesn't allow memory-to-memory moves
                let is_src_mem = matches!(s_op, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::FloatMem(..));
                let is_dest_mem = matches!(d_op, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::FloatMem(..));
                
                if is_src_mem || (is_dest_mem && !matches!(s_op, X86Operand::Imm(..) | X86Operand::Reg(..))) {
                    // Use intermediate register for memory operands or complex addressing
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                } else if matches!(s_op, X86Operand::Label(..)) {
                    // This shouldn't happen now that we handle Global above
                    // But just in case, use LEA for any remaining labels
                    if let X86Operand::Label(name) = s_op {
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                } else {
                    self.asm.push(X86Instr::Mov(d_op, s_op));
                }
            }
            IrInstruction::Binary { dest, op, left, right } => {
                self.gen_binary_op(*dest, op, left, right);
            }
            IrInstruction::FloatBinary { dest, op, left, right } => {
                self.gen_float_binary_op(*dest, op, left, right);
            }
            IrInstruction::Unary { dest, op, src } => {
                self.gen_unary_op(*dest, op, src);
            }
            IrInstruction::FloatUnary { dest, op, src } => {
                self.gen_float_unary_op(*dest, op, src);
            }
            IrInstruction::Phi { .. } => {}
            IrInstruction::Alloca { dest, r#type } => {
                // Record that Alloca result is a pointer
                self.var_types.insert(*dest, Type::Pointer(Box::new(r#type.clone())));
                // No code generation needed - buffer offset already tracked in alloca_buffers
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

    fn gen_binary_op(&mut self, dest: VarId, op: &BinaryOp, left: &Operand, right: &Operand) {
        let l_op = self.operand_to_op(left);
        let r_op = self.operand_to_op(right);
        let d_op = self.var_to_op(dest);
        InstructionGenerator::gen_binary_op(
            &mut self.asm,
            dest,
            op,
            l_op,
            r_op,
            d_op,
        );
    }

    fn gen_unary_op(&mut self, dest: VarId, op: &UnaryOp, src: &Operand) {
        let s_op = self.operand_to_op(src);
        let d_op = self.var_to_op(dest);
        InstructionGenerator::gen_unary_op(
            &mut self.asm,
            dest,
            op,
            s_op,
            d_op,
        );
    }

    fn gen_float_binary_op(&mut self, dest: VarId, op: &BinaryOp, left: &Operand, right: &Operand) {
        let _d_op = self.var_to_op(dest);
        
        // Load left operand into xmm0
        match left {
            Operand::FloatConstant(_) => {
                let left_label = self.operand_to_op(left);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_label));
            }
            Operand::Var(v) => {
                let left_op = self.var_to_op(*v);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_op));
            }
            Operand::Constant(c) => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                self.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        // Load right operand into xmm1
        match right {
            Operand::FloatConstant(_) => {
                let right_label = self.operand_to_op(right);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_label));
            }
            Operand::Var(v) => {
                let right_op = self.var_to_op(*v);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_op));
            }
            Operand::Constant(c) => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                self.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        // Perform operation
        match op {
            BinaryOp::Add => {
                self.var_types.insert(dest, Type::Float);
                self.asm.push(X86Instr::Addss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Sub => {
                self.var_types.insert(dest, Type::Float);
                self.asm.push(X86Instr::Subss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Mul => {
                self.var_types.insert(dest, Type::Float);
                self.asm.push(X86Instr::Mulss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Div => {
                self.var_types.insert(dest, Type::Float);
                self.asm.push(X86Instr::Divss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::EqualEqual | BinaryOp::NotEqual => {
                self.var_types.insert(dest, Type::Int);
                // Float comparison: ucomiss sets flags
                self.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                
                // Set condition based on flags (result as 0 or 1 integer)
                let cond = match op {
                    BinaryOp::Less => "b",
                    BinaryOp::LessEqual => "be",
                    BinaryOp::Greater => "a",
                    BinaryOp::GreaterEqual => "ae",
                    BinaryOp::EqualEqual => "e",
                    BinaryOp::NotEqual => "ne",
                    _ => unreachable!(),
                };
                self.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
                
                // Since result is Int, d_op will be QWORD PTR (unless mapped to Reg), so Mov Rax is valid.
                // Re-get d_op because var_types changed? 
                // var_to_op uses var_types. If we just inserted Int, it will return Mem (64-bit) instead of FloatMem (32-bit).
                // But we called var_to_op AT THE START of the function!
                // So d_op holds the OLD value (likely FloatMem because we inserted Float at the top in previous version, or nothing if not seen).
                // Wait, previously `self.var_types.insert(dest, Type::Float)` was at line 466. 
                // So `d_op` (line 468) used that. 
                // Now I removed it. `d_op` might be computed without type info -> defaults to Mem (64-bit).
                // Or if it was seen before? It's SSA, so it's new.
                // So d_op should be Mem.
                // BUT, I should recompute d_op or move var_to_op call.
                
                let dest_op = self.var_to_op(dest); 
                self.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
                return; 
            }
            _ => {
                // Unsupported float operation - just store 0
                self.var_types.insert(dest, Type::Float);
                self.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
            }
        }
        
        // Store result from xmm0 to destination (for arithmetic ops)
        // d_op calculated earlier might be wrong if we didn't insert type?
        // If undefined, defaults to Mem.
        // For float ops, we want FloatMem.
        // So we should re-calculate d_op or insert type earlier.
        
        let dest_op = self.var_to_op(dest);
        self.asm.push(X86Instr::Movss(dest_op, X86Operand::Reg(X86Reg::Xmm0)));
    }

    fn gen_float_unary_op(&mut self, dest: VarId, op: &UnaryOp,src: &Operand) {
        // Record that result is a float
        self.var_types.insert(dest, Type::Float);
        
        let d_op = self.var_to_op(dest);
        
        // Load source into xmm0
        match src {
            Operand::FloatConstant(_) => {
                let src_label = self.operand_to_op(src);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_label));
            }
            Operand::Var(v) => {
                let src_op = self.var_to_op(*v);
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_op));
            }
            Operand::Constant(c) => {
                // Convert int to float
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                self.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        match op {
            UnaryOp::Minus => {
                // Negate by XORing with sign bit (0x80000000)
                let sign_bit_label = self.get_or_create_float_const(f64::from_bits(0x8000000000000000u64));
                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::RipRelLabel(sign_bit_label)));
                self.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            UnaryOp::LogicalNot => {
                // Convert float to int, test if zero, return 1 or 0
                self.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(X86Reg::Eax), X86Operand::Reg(X86Reg::Xmm0)));
                self.asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(0)));
                self.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                return; // Return int result, not float
            }
            _ => {
                // Unsupported - return 0
                self.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
            }
        }
        
        // Store result from xmm0 to destination
        self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    }

    fn gen_load(&mut self, dest: VarId, addr: &Operand, value_type: &model::Type) {
        self.var_types.insert(dest, value_type.clone());
        let d_op = self.var_to_op(dest);
        let is_float = matches!(value_type, Type::Float | Type::Double);
        let use_dword = matches!(value_type, Type::Int | Type::Float);
        
        if is_float {
            match addr {
                Operand::Global(name) => {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rax, 0)));
                    self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                }
                Operand::Var(var) => {
                    // Check if this is an alloca - use direct memory access
                    if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                        self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, *buffer_offset)));
                        self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                    } else {
                        let a_op = self.var_to_op(*var);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                        self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rax, 0)));
                        self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                    }
                }
                Operand::Constant(addr_const) => {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*addr_const)));
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rax, 0)));
                    self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                }
                Operand::FloatConstant(_) => {
                    self.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
                }
            }
        } else {
            match addr {
                Operand::Global(name) => {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    if use_dword {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
                        self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
                    }
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
                Operand::Var(var) => {
                    // Check if this is an alloca - use direct memory access
                    if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                        if use_dword {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rbp, *buffer_offset)));
                            self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                        } else {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                        }
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    } else {
                        let a_op = self.var_to_op(*var);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                        if use_dword {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
                            self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                        } else {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
                        }
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                }
                Operand::Constant(addr_const) => {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*addr_const)));
                    if use_dword {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
                        self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
                    }
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
                Operand::FloatConstant(_) => {
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Imm(0)));
                }
            }
        }
    }

    fn gen_store(&mut self, addr: &Operand, src: &Operand, value_type: &model::Type) {
        let use_dword = matches!(value_type, Type::Int | Type::Float);
        let is_float = matches!(value_type, Type::Float | Type::Double);
        
        // Special handling for float constants
        if let Operand::FloatConstant(_) = src {
            let label = self.operand_to_op(src);
            self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
            
            match addr {
                Operand::Var(var) => {
                    // Check if this is an alloca - use direct memory access
                    if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                        self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rbp, *buffer_offset), X86Operand::Reg(X86Reg::Xmm0)));
                    } else {
                        let a_op = self.var_to_op(*var);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                        self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
                    }
                }
                Operand::Global(name) => {
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
                }
                _ => {}
            }
            return;
        }
        
        // Load source into appropriate register
        if let Operand::Global(func_name) = src {
            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::RipRelLabel(func_name.clone())));
        } else if is_float {
            match src {
                Operand::Var(v) => {
                    let src_op = self.var_to_op(*v);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_op));
                }
                _ => {
                    let s_op = self.operand_to_op(src);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                }
            }
        } else {
            // Check if source is an alloca (array decay to pointer)
            if let Operand::Var(src_var) = src {
                if let Some(buffer_offset) = self.alloca_buffers.get(src_var) {
                    // This is an alloca - get its address with LEA
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                } else {
                    let s_op = self.operand_to_op(src);
                    // Check if s_op references an alloca buffer offset
                    if let X86Operand::Mem(X86Reg::Rbp, offset) = s_op {
                        let is_alloca_buffer = self.alloca_buffers.values().any(|&buf_offset| buf_offset == offset);
                        if is_alloca_buffer {
                            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::Mem(X86Reg::Rbp, offset)));
                        } else {
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
                        }
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
                    }
                }
            } else {
                let s_op = self.operand_to_op(src);
                // Check if s_op references an alloca buffer offset
                if let X86Operand::Mem(X86Reg::Rbp, offset) = s_op {
                    let is_alloca_buffer = self.alloca_buffers.values().any(|&buf_offset| buf_offset == offset);
                    if is_alloca_buffer {
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::Mem(X86Reg::Rbp, offset)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
                    }
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
                }
            }
        }

        match addr {
            Operand::Global(name) => {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                if is_float {
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
                } else if use_dword {
                    self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
                }
            }
            Operand::Var(var) => {
                // Check if this is an alloca - use direct memory access
                if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                    if is_float {
                        self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rbp, *buffer_offset), X86Operand::Reg(X86Reg::Xmm0)));
                    } else if use_dword {
                        self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rbp, *buffer_offset), X86Operand::Reg(X86Reg::Ecx)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rbp, *buffer_offset), X86Operand::Reg(X86Reg::Rcx)));
                    }
                } else {
                    let a_op = self.var_to_op(*var);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                    if is_float {
                        self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
                    } else if use_dword {
                        self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
                    }
                }
            }
            Operand::Constant(addr_const) => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*addr_const)));
                if is_float {
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
                } else if use_dword {
                    self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
                }
            }
            Operand::FloatConstant(_) => {}
        }
    }

    fn gen_gep(&mut self, dest: VarId, base: &Operand, index: &Operand, element_type: &Type) {
        let i_op = self.operand_to_op(index);
        let d_op = self.var_to_op(dest);
        let elem_size = self.get_type_size(element_type) as i64;
        
        match base {
            Operand::Global(name) => {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Label(name.clone())));
            }
            Operand::Var(var) => {
                // Check if this is an alloca (array) - get its address, not its value
                if let Some(buffer_offset) = self.alloca_buffers.get(var) {
                    // This is an alloca - LEA to get the buffer address
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                } else {
                    // Regular pointer variable - load its value
                    let b_op = self.var_to_op(*var);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
                }
            }
            Operand::Constant(c) => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*c)));
            }
            Operand::FloatConstant(_) => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
            }
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
                    // Special handling for Global operands (string literals, function pointers)
                    if let Operand::Global(name) = arg {
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(param_regs[i].clone()), X86Operand::RipRelLabel(name.clone())));
                    } else {
                        let val = self.operand_to_op(arg);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(param_regs[i].clone()), val));
                    }
                }
            } else {
                let offset = 32 + (i - 4) * 8;
                if is_float {
                    let label = self.operand_to_op(arg);
                    self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                    self.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    // Special handling for Global operands
                    if let Operand::Global(name) = arg {
                        self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    } else {
                        let val = self.operand_to_op(arg);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
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
        
        let fp_op = self.operand_to_op(func_ptr);
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), fp_op));
        
        for (i, arg) in args.iter().enumerate() {
            let val = self.operand_to_op(arg);
            let target_reg_or_mem = if i < 4 {
                Some(X86Operand::Reg(param_regs[i].clone()))
            } else {
                None
            };

            if let Some(target) = target_reg_or_mem {
                self.asm.push(X86Instr::Mov(target, val));
            } else {
                let offset = 32 + (i - 4) * 8;
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
            }
        }
        
        self.asm.push(X86Instr::CallIndirect(X86Operand::Reg(X86Reg::R10)));
        
        if let Some(d) = dest {
            let d_op = self.var_to_op(*d);
            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }

    fn gen_inline_asm(&mut self, template: &str, outputs: &[VarId], inputs: &[Operand], _clobbers: &[String], _is_volatile: bool) {
        // For basic inline assembly support, we just emit the template as raw assembly
        // More advanced constraint handling would require parsing the template and
        // substituting %0, %1, etc. with actual operands
        
        // Convert AT&T syntax placeholders (%0, %1) to actual operands
        let mut asm_code = template.to_string();
        
        // Replace output placeholders
        // For outputs that map to alloca variables (stored as pointers),
        // we need to load the pointer first and use dereferenced form
        for (i, output_var) in outputs.iter().enumerate() {
            let placeholder = format!("%{}", i);
            let output_op = self.var_to_op(*output_var);
            
            // For memory operands, check if we need to dereference
            // Alloca variables are stored as pointers in memory, so we load and deref
            let operand_str = match &output_op {
                X86Operand::Mem(_reg, _offset) => {
                    // This might be a pointer to the actual value
                    // Load it into rax and use dereferenced form
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), output_op.clone()));
                    format!("DWORD PTR [rax]")
                }
                X86Operand::DwordMem(reg, offset) => {
                    if *offset == 0 {
                        format!("DWORD PTR [{}]", reg.to_str())
                    } else {
                        format!("DWORD PTR [{}{}]", reg.to_str(), 
                            if *offset > 0 { format!("+{}", offset) } else { offset.to_string() })
                    }
                }
                X86Operand::Reg(r) => r.to_str().to_string(),
                _ => "rax".to_string(), // Fallback
            };
            
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        
        // Replace input placeholders (they come after outputs)
        for (i, input_op) in inputs.iter().enumerate() {
            let placeholder = format!("%{}", outputs.len() + i);
            let input_x86_op = self.operand_to_op(input_op);
            
            let operand_str = match &input_x86_op {
                X86Operand::Imm(val) => val.to_string(),
                X86Operand::Reg(r) => r.to_str().to_string(),
                X86Operand::Mem(reg, offset) => {
                    if *offset == 0 {
                        format!("DWORD PTR [{}]", reg.to_str())
                    } else {
                        format!("DWORD PTR [{}{}]", reg.to_str(), 
                            if *offset > 0 { format!("+{}", offset) } else { offset.to_string() })
                    }
                }
                _ => "0".to_string(),
            };
            
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        
        // Convert GCC inline assembly escape syntax {$} to nothing for Intel syntax
        // In Intel syntax, immediates don't use $ prefix (unlike AT&T syntax)
        asm_code = asm_code.replace("{$}", "");
        
        // Emit as raw assembly
        self.asm.push(X86Instr::Raw(asm_code));
    }

    fn get_type_size(&self, r#type: &model::Type) -> usize {
        let calculator = TypeCalculator::new(&self.structs, &self.unions);
        calculator.get_type_size(r#type)
    }



    fn gen_terminator(&mut self, term: &IrTerminator, func_name: &str, func: &IrFunction) {
        match term {
            IrTerminator::Ret(op) => {
                if let Some(o) = op {
                    let is_float_return = matches!(func.return_type, Type::Float | Type::Double);
                    
                    if is_float_return {
                        match o {
                            Operand::FloatConstant(_) => {
                                let label = self.operand_to_op(o);
                                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                            }
                            Operand::Var(v) => {
                                let val = self.var_to_op(*v);
                                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), val));
                            }
                            _ => {
                                let val = self.operand_to_op(o);
                                self.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), val));
                            }
                        }
                    } else {
                        let val = self.operand_to_op(o);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                    }
                }
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rsp), X86Operand::Reg(X86Reg::Rbp)));
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
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rsp), X86Operand::Reg(X86Reg::Rbp)));
                self.asm.push(X86Instr::Pop(X86Reg::Rbp));
                self.asm.push(X86Instr::Ret);
            }
        }
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
                        // Check if source is an alloca (array decay to pointer in phi)
                        if let Some(buffer_offset) = self.alloca_buffers.get(src_var) {
                            // Get address of alloca with LEA
                            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                        } else {
                            // Regular variable - load its value
                            let s_op = self.var_to_op(*src_var);
                            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                        }
                    }
                }
            }
        }
    }
}
