mod x86;
mod regalloc;
mod peephole;

use model::{BinaryOp, UnaryOp, Type};
use ir::{IRProgram, Function as IrFunction, BlockId, VarId, Operand, Instruction as IrInstruction, Terminator as IrTerminator};
use std::collections::HashMap;

pub use x86::{X86Reg, X86Operand, X86Instr, emit_asm};
pub use regalloc::{PhysicalReg, allocate_registers};
use peephole::apply_peephole;

pub struct Codegen {
    // SSA Var -> Stack Offset (for spills or non-register vars)
    stack_slots: HashMap<VarId, i32>,
    next_slot: i32,
    asm: Vec<X86Instr>,
    structs: HashMap<String, model::StructDef>,
    // Register allocation
    reg_alloc: HashMap<VarId, PhysicalReg>,
    enable_regalloc: bool,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            stack_slots: HashMap::new(),
            next_slot: 0,
            asm: Vec::new(),
            structs: HashMap::new(),
            reg_alloc: HashMap::new(),
            enable_regalloc: true, // Enable register allocation by default
        }
    }

    pub fn gen_program(&mut self, prog: &IRProgram) -> String {
        self.structs.clear();
        for s_def in &prog.structs {
            self.structs.insert(s_def.name.clone(), s_def.clone());
        }
        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n");
        
        // Strings
        if !prog.global_strings.is_empty() {
            output.push_str(".data\n");
            for (label, content) in &prog.global_strings {
                let escaped = content.replace("\"", "\\\"");
                output.push_str(&format!("{}: .asciz \"{}\"\n", label, escaped));
            }
        }

        // Globals (use .quad for all to simplify codegen - int values will be sign-extended)
        if !prog.globals.is_empty() {
             if prog.global_strings.is_empty() { output.push_str(".data\n"); }
             for g in &prog.globals {
                 output.push_str(&format!(".globl {}\n", g.name));
                 output.push_str(&format!(".align 4\n"));  // Ensure 4-byte alignment for ints
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
             }
        }

        output.push_str(".text\n");
        output.push_str(".globl main\n\n");
        
        for func in &prog.functions {
            self.gen_function(func);
            
            // Apply peephole optimizations
            apply_peephole(&mut self.asm);
            
            output.push_str(&emit_asm(&self.asm));
            self.asm.clear();
            self.stack_slots.clear();
            self.reg_alloc.clear();
            self.next_slot = 0;
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

        // Handle parameters (Windows ABI: RCX, RDX, R8, R9)
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
        for (i, (_, var)) in func.params.iter().enumerate() {
            let dest = self.var_to_op(*var);
            if i < 4 {
                self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(param_regs[i].clone())));
            } else {
                let offset = 16 + 32 + (i - 4) * 8;
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, offset as i32)));
                self.asm.push(X86Instr::Mov(dest, X86Operand::Reg(X86Reg::Rax)));
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
                        // Allocas always need stack space
                        // First allocate slot for pointer variable
                        self.next_slot += 8;
                        let ptr_slot = -self.next_slot;
                        self.stack_slots.insert(*dest, ptr_slot);
                        
                        // Then allocate buffer space (aligned to 8 bytes)
                        let size = self.get_type_size(r#type) as i32;
                        let aligned_size = ((size + 7) / 8) * 8;
                        self.next_slot += aligned_size;
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
            Operand::FloatConstant(_f) => {
                // TODO: Handle float constants properly by loading from data section
                // For now, return a placeholder
                X86Operand::Imm(0)
            }
            Operand::Var(v) => self.var_to_op(*v),
            Operand::Global(s) => X86Operand::Label(s.clone()),
        }
    }

    fn gen_instr(&mut self, inst: &IrInstruction) {
        match inst {
            IrInstruction::Copy { dest, src } => {
                let d_op = self.var_to_op(*dest);
                // Special handling for Global sources (function addresses, globals)
                if let Operand::Global(name) = src {
                    // Use LEA with RIP-relative addressing to get the address
                    self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    return;
                }
                
                let s_op = self.operand_to_op(src);
                if matches!(s_op, X86Operand::Mem(..) | X86Operand::DwordMem(..)) {
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
                let size = self.get_type_size(r#type);
                let aligned_size = ((size + 7) / 8) * 8;  // Align to 8 bytes
                let ptr_slot = *self.stack_slots.get(dest).expect("alloca dest must have slot");
                let buffer_offset = ptr_slot - aligned_size as i32;
                let d_op = self.var_to_op(*dest);
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, buffer_offset)));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            IrInstruction::Load { dest, addr } => {
                self.gen_load(*dest, addr);
            }
            IrInstruction::Store { addr, src } => {
                self.gen_store(addr, src);
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
        }
    }

    fn gen_binary_op(&mut self, dest: VarId, op: &BinaryOp, left: &Operand, right: &Operand) {
        let l_op = self.operand_to_op(left);
        let r_op = self.operand_to_op(right);
        let d_op = self.var_to_op(dest);
        
        match op {
            BinaryOp::Add => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::Add(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Sub => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::Sub(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Mul => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::Imul(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Div => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                self.asm.push(X86Instr::Cqto);
                if let X86Operand::Imm(_) = r_op {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    self.asm.push(X86Instr::Idiv(X86Operand::Reg(X86Reg::Rcx)));
                } else {
                    self.asm.push(X86Instr::Idiv(r_op));
                }
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            BinaryOp::Mod => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                self.asm.push(X86Instr::Cqto);
                if let X86Operand::Imm(_) = r_op {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    self.asm.push(X86Instr::Idiv(X86Operand::Reg(X86Reg::Rcx)));
                } else {
                    self.asm.push(X86Instr::Idiv(r_op));
                }
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rdx)));
            }
            BinaryOp::EqualEqual | BinaryOp::NotEqual | BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                self.asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Rax), r_op));
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                let cond = match op {
                    BinaryOp::EqualEqual => "e",
                    BinaryOp::NotEqual => "ne",
                    BinaryOp::Less => "l",
                    BinaryOp::LessEqual => "le",
                    BinaryOp::Greater => "g",
                    BinaryOp::GreaterEqual => "ge",
                    _ => unreachable!(),
                };
                self.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            BinaryOp::BitwiseAnd => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::And(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::BitwiseOr => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::Or(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Or(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::BitwiseXor => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    self.asm.push(X86Instr::Xor(d_op, r_op));
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Xor(X86Operand::Reg(X86Reg::Rax), r_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::ShiftLeft => {
                if let X86Operand::Imm(shift) = r_op {
                    if matches!(d_op, X86Operand::Reg(_)) {
                        self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        self.asm.push(X86Instr::Shl(d_op, X86Operand::Imm(shift)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        self.asm.push(X86Instr::Shl(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(shift)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    self.asm.push(X86Instr::Shl(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::ShiftRight => {
                if let X86Operand::Imm(shift) = r_op {
                    if matches!(d_op, X86Operand::Reg(_)) {
                        self.asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        self.asm.push(X86Instr::Shr(d_op, X86Operand::Imm(shift)));
                    } else {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        self.asm.push(X86Instr::Shr(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(shift)));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                } else {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    self.asm.push(X86Instr::Shr(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            _ => {}
        }
    }

    fn gen_unary_op(&mut self, dest: VarId, op: &UnaryOp, src: &Operand) {
        let s_op = self.operand_to_op(src);
        let d_op = self.var_to_op(dest);
        match op {
            UnaryOp::Minus => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), s_op));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::LogicalNot => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                self.asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                self.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::BitwiseNot => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                self.asm.push(X86Instr::Not(X86Operand::Reg(X86Reg::Rax)));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::Plus => {
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::AddrOf | UnaryOp::Deref => unreachable!("AddrOf and Deref should be lowered by IR"),
        }
    }

    fn gen_float_binary_op(&mut self, _dest: VarId, _op: &BinaryOp, _left: &Operand, _right: &Operand) {
        // TODO: Implement float binary operations using XMM registers
        // For now, do nothing - this will produce incorrect results but allows compilation
    }

    fn gen_float_unary_op(&mut self, _dest: VarId, _op: &UnaryOp, _src: &Operand) {
        // TODO: Implement float unary operations using XMM registers
        // For now, do nothing - this will produce incorrect results but allows compilation
    }

    fn gen_load(&mut self, dest: VarId, addr: &Operand) {
        let d_op = self.var_to_op(dest);
        match addr {
            Operand::Global(name) => {
                 // Load address into a register using RIP-relative LEA, then access through it
                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
                 // movsx rax, eax to sign-extend 32-bit to 64-bit
                 self.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                 self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            Operand::Var(var) => {
                 let a_op = self.var_to_op(*var);
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
                 self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            Operand::Constant(addr_const) => {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*addr_const)));
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
                 self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            Operand::FloatConstant(_f) => {
                // TODO: Handle float constant loads properly
                self.asm.push(X86Instr::Mov(d_op, X86Operand::Imm(0)));
            }
        }
    }

    fn gen_store(&mut self, addr: &Operand, src: &Operand) {
        // Special handling for Global sources (function pointers) - need LEA not MOV
        if let Operand::Global(func_name) = src {
            self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::RipRelLabel(func_name.clone())));
        } else {
            let s_op = self.operand_to_op(src);
            self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
        }

        match addr {
            Operand::Global(name) => {
                 // Load address into a register using RIP-relative LEA, then store through it
                 self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                 self.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
            }
            Operand::Var(var) => {
                 let a_op = self.var_to_op(*var);
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), a_op));
                 self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
            }
            Operand::Constant(addr_const) => {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*addr_const)));
                 self.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
            }
            Operand::FloatConstant(_f) => {
                // TODO: Handle float constant stores properly
                // For now, do nothing
            }
        }
    }

    fn gen_gep(&mut self, dest: VarId, base: &Operand, index: &Operand, element_type: &Type) {
        let i_op = self.operand_to_op(index);
        let d_op = self.var_to_op(dest);
        let elem_size = self.get_element_size(element_type) as i64;
        
        // Calculate base into RAX
        match base {
            Operand::Global(name) => {
                self.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Label(name.clone())));
            }
            Operand::Var(var) => {
                 let b_op = self.var_to_op(*var);
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
            }
            Operand::Constant(c) => {
                 self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(*c)));
            }
            Operand::FloatConstant(_f) => {
                // TODO: Properly handle float constants
                self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
            }
        }

        // Calculate Index with proper element size
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), i_op));
        if elem_size != 1 {
            self.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rcx), X86Operand::Imm(elem_size)));
        }
        self.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    }

    fn gen_call(&mut self, dest: &Option<VarId>, name: &str, args: &[Operand]) {
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
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
        self.asm.push(X86Instr::Call(name.to_string()));
        if let Some(d) = dest {
            let d_op = self.var_to_op(*d);
            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }

    fn gen_indirect_call(&mut self, dest: &Option<VarId>, func_ptr: &Operand, args: &[Operand]) {
        let param_regs = [X86Reg::Rcx, X86Reg::Rdx, X86Reg::R8, X86Reg::R9];
        
        // First, save the function pointer to a safe location (R10)
        let fp_op = self.operand_to_op(func_ptr);
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), fp_op));
        
        // Now set up arguments
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
        
        // Indirect call through R10
        self.asm.push(X86Instr::CallIndirect(X86Operand::Reg(X86Reg::R10)));
        
        if let Some(d) = dest {
            let d_op = self.var_to_op(*d);
            self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }

    fn get_type_size(&self, r#type: &model::Type) -> usize {
        match r#type {
            model::Type::Int => 8,  // Use 8 bytes for consistency with pointers
            model::Type::Void => 0,
            model::Type::Float => 4,  // 32-bit float
            model::Type::Double => 8, // 64-bit double
            model::Type::Array(inner, size) => self.get_type_size(inner) * size,
            model::Type::Pointer(_) => 8,
            model::Type::FunctionPointer { .. } => 8,
            model::Type::Char => 1,
            model::Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let mut size = 0;
                    for (f_ty, _) in &s_def.fields {
                        size += self.get_type_size(f_ty);
                    }
                    size
                } else {
                    8
                }
            }
            model::Type::Typedef(_) => 8,
        }
    }

    fn get_element_size(&self, r#type: &model::Type) -> usize {
        match r#type {
            model::Type::Int => 8,  // Use 8 bytes for consistency
            model::Type::Void => 0,
            model::Type::Float => 4,
            model::Type::Double => 8,
            model::Type::Array(inner, size) => self.get_element_size(inner) * size,
            model::Type::Pointer(_) => 8,
            model::Type::FunctionPointer { .. } => 8,
            model::Type::Char => 1,
            model::Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let mut size = 0;
                    for (f_ty, _) in &s_def.fields {
                        size += self.get_element_size(f_ty);
                    }
                    size
                } else {
                    8
                }
            }
            model::Type::Typedef(_) => 8,
        }
    }

    fn gen_terminator(&mut self, term: &IrTerminator, func_name: &str, func: &IrFunction) {
        match term {
            IrTerminator::Ret(op) => {
                if let Some(o) = op {
                    let val = self.operand_to_op(o);
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
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
                
                self.asm.push(X86Instr::Cmp(c_op, X86Operand::Imm(0)));
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
                        let s_op = self.var_to_op(*src_var);
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                }
            }
        }
    }
}
