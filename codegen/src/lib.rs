use model::{BinaryOp, UnaryOp, Type};
use ir::{IRProgram, Function as IrFunction, BlockId, VarId, Operand as IrOperand, Instruction as IrInstruction, Terminator as IrTerminator};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum X86Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Al, // 8-bit rax
}

impl X86Reg {
    fn to_str(&self) -> &str {
        match self {
            Self::Rax => "rax", Self::Rcx => "rcx", Self::Rdx => "rdx", Self::Rbx => "rbx",
            Self::Rsp => "rsp", Self::Rbp => "rbp", Self::Rsi => "rsi", Self::Rdi => "rdi",
            Self::R8 => "r8", Self::R9 => "r9", Self::R10 => "r10", Self::R11 => "r11",
            Self::R12 => "r12", Self::R13 => "r13", Self::R14 => "r14", Self::R15 => "r15",
            Self::Al => "al",
        }
    }
}

#[derive(Debug, Clone)]
pub enum X86Operand {
    Reg(X86Reg),
    Mem(X86Reg, i32), // [reg + offset]
    Imm(i64),
    Label(String),
}

impl X86Operand {
    fn to_string(&self) -> String {
        match self {
            Self::Reg(r) => r.to_str().to_string(),
            Self::Mem(r, offset) => format!("QWORD PTR [{}{:+}]", r.to_str(), offset),
            Self::Imm(i) => i.to_string(),
            Self::Label(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum X86Instr {
    Mov(X86Operand, X86Operand),
    Add(X86Operand, X86Operand),
    Sub(X86Operand, X86Operand),
    Imul(X86Operand, X86Operand),
    Idiv(X86Operand),
    Cmp(X86Operand, X86Operand),
    Set(String, X86Operand), // e.g., sete, setl
    Jmp(String),
    Jcc(String, String), // condition, label
    Push(X86Reg),
    Pop(X86Reg),
    Call(String),
    Ret,
    Label(String),
    Cqto, // CDQ/CQO for division
    Xor(X86Operand, X86Operand),
}

pub struct Codegen {
    // SSA Var -> Stack Offset
    stack_slots: HashMap<VarId, i32>,
    next_slot: i32,
    asm: Vec<X86Instr>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            stack_slots: HashMap::new(),
            next_slot: 0,
            asm: Vec::new(),
        }
    }

    pub fn gen_program(&mut self, prog: &IRProgram) -> String {
        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n");
        output.push_str(".globl main\n\n");
        
        for func in &prog.functions {
            self.gen_function(func);
            output.push_str(&self.emit_asm());
            self.asm.clear();
            self.stack_slots.clear();
            self.next_slot = 0;
        }
        
        output
    }

    fn gen_function(&mut self, func: &IrFunction) {
        self.asm.push(X86Instr::Label(func.name.clone()));
        
        // Prologue
        self.asm.push(X86Instr::Push(X86Reg::Rbp));
        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rbp), X86Operand::Reg(X86Reg::Rsp)));
        
        self.allocate_stack_slots(func);
        // Windows requires 32 bytes shadow space for calls, plus we need space for locals.
        // Stack must be 16-byte aligned.
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
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    IrInstruction::Binary { dest, .. } |
                    IrInstruction::Unary { dest, .. } |
                    IrInstruction::Phi { dest, .. } |
                    IrInstruction::Copy { dest, .. } => {
                        self.get_or_create_slot(*dest);
                    }
                }
            }
        }
        for (_, var) in &func.params {
            self.get_or_create_slot(*var);
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
        let slot = self.get_or_create_slot(var);
        X86Operand::Mem(X86Reg::Rbp, slot)
    }

    fn operand_to_op(&mut self, op: &IrOperand) -> X86Operand {
        match op {
            IrOperand::Constant(c) => X86Operand::Imm(*c),
            IrOperand::Var(v) => self.var_to_op(*v),
        }
    }

    fn gen_instr(&mut self, inst: &IrInstruction) {
        match inst {
            IrInstruction::Copy { dest, src } => {
                let d_op = self.var_to_op(*dest);
                let s_op = self.operand_to_op(src);
                if matches!(s_op, X86Operand::Mem(..)) {
                    self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                    self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                } else {
                    self.asm.push(X86Instr::Mov(d_op, s_op));
                }
            }
            IrInstruction::Binary { dest, op, left, right } => {
                let l_op = self.operand_to_op(left);
                let r_op = self.operand_to_op(right);
                let d_op = self.var_to_op(*dest);
                
                match op {
                    BinaryOp::Add => {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        self.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), r_op));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                    BinaryOp::Sub => {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        self.asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), r_op));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                    BinaryOp::Mul => {
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        self.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rax), r_op));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
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
                    _ => {}
                }
            }
            IrInstruction::Unary { dest, op, src } => {
                let s_op = self.operand_to_op(src);
                let d_op = self.var_to_op(*dest);
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
                    _ => {}
                }
            }
            IrInstruction::Phi { .. } => {}
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
        // This is a bit hacky, but since we iterate in order, it's the last label we added.
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
                        // mov cannot have both operands as memory
                        self.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                        self.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                }
            }
        }
    }

    fn emit_asm(&self) -> String {
        let mut s = String::new();
        for instr in &self.asm {
            match instr {
                X86Instr::Label(l) => s.push_str(&format!("{}:\n", l)),
                X86Instr::Mov(d, src) => s.push_str(&format!("  mov {}, {}\n", d.to_string(), src.to_string())),
                X86Instr::Add(d, src) => s.push_str(&format!("  add {}, {}\n", d.to_string(), src.to_string())),
                X86Instr::Sub(d, src) => s.push_str(&format!("  sub {}, {}\n", d.to_string(), src.to_string())),
                X86Instr::Imul(d, src) => s.push_str(&format!("  imul {}, {}\n", d.to_string(), src.to_string())),
                X86Instr::Idiv(src) => s.push_str(&format!("  idiv {}\n", src.to_string())),
                X86Instr::Cmp(l, r) => s.push_str(&format!("  cmp {}, {}\n", l.to_string(), r.to_string())),
                X86Instr::Set(c, d) => s.push_str(&format!("  set{} {}\n", c, d.to_string())),
                X86Instr::Jmp(l) => s.push_str(&format!("  jmp {}\n", l)),
                X86Instr::Jcc(c, l) => s.push_str(&format!("  j{} {}\n", c, l)),
                X86Instr::Push(r) => s.push_str(&format!("  push {}\n", r.to_str())),
                X86Instr::Pop(r) => s.push_str(&format!("  pop {}\n", r.to_str())),
                X86Instr::Call(l) => s.push_str(&format!("  call {}\n", l)),
                X86Instr::Ret => s.push_str("  ret\n"),
                X86Instr::Cqto => s.push_str("  cqo\n"),
                X86Instr::Xor(d, s_op) => s.push_str(&format!("  xor {}, {}\n", d.to_string(), s_op.to_string())),
            }
        }
        s
    }
}
