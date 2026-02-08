// X86-64 register and instruction definitions

#[derive(Debug, Clone)]
pub enum X86Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Eax, Ecx, // 32-bit registers
    Al, // 8-bit rax
}

impl X86Reg {
    pub fn to_str(&self) -> &str {
        match self {
            Self::Rax => "rax", Self::Rcx => "rcx", Self::Rdx => "rdx", Self::Rbx => "rbx",
            Self::Rsp => "rsp", Self::Rbp => "rbp", Self::Rsi => "rsi", Self::Rdi => "rdi",
            Self::R8 => "r8", Self::R9 => "r9", Self::R10 => "r10", Self::R11 => "r11",
            Self::R12 => "r12", Self::R13 => "r13", Self::R14 => "r14", Self::R15 => "r15",
            Self::Eax => "eax", Self::Ecx => "ecx",
            Self::Al => "al",
        }
    }
}

#[derive(Debug, Clone)]
pub enum X86Operand {
    Reg(X86Reg),
    Mem(X86Reg, i32), // [reg + offset] - QWORD PTR
    DwordMem(X86Reg, i32), // [reg + offset] - DWORD PTR (32-bit)
    Imm(i64),
    Label(String),
    GlobalMem(String), // RIP-relative global: label[rip]
    RipRelLabel(String), // For LEA: label[rip]
}

impl X86Operand {
    pub fn to_string(&self) -> String {
        match self {
            Self::Reg(r) => r.to_str().to_string(),
            Self::Mem(r, offset) => format!("QWORD PTR [{}{:+}]", r.to_str(), offset),
            Self::DwordMem(r, offset) => format!("DWORD PTR [{}{:+}]", r.to_str(), offset),
            Self::Imm(i) => i.to_string(),
            Self::Label(s) => s.clone(), // Just emit the label as-is (for LEA)
            Self::GlobalMem(name) => format!("DWORD PTR {}[rip]", name), // RIP-relative 32-bit int access
            Self::RipRelLabel(name) => format!("{}[rip]", name), // RIP-relative label for LEA
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
    Set(String, X86Operand),
    Jmp(String),
    Jcc(String, String),
    Push(X86Reg),
    Pop(X86Reg),
    Call(String),
    Ret,
    Label(String),
    Cqto,
    Xor(X86Operand, X86Operand),
    Lea(X86Operand, X86Operand),
    And(X86Operand, X86Operand),
    Or(X86Operand, X86Operand),
    Not(X86Operand),
    Shl(X86Operand, X86Operand),
    Shr(X86Operand, X86Operand),
    Movsx(X86Operand, X86Operand), // Sign-extend smaller value into larger register
}

/// emit_asm converts X86 instructions to AT&T syntax assembly
pub fn emit_asm(instructions: &[X86Instr]) -> String {
    let mut s = String::new();
    for instr in instructions {
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
            X86Instr::Lea(d, s_op) => s.push_str(&format!("  lea {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::And(d, s_op) => s.push_str(&format!("  and {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::Or(d, s_op) => s.push_str(&format!("  or {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::Not(d) => s.push_str(&format!("  not {}\n", d.to_string())),
            X86Instr::Shl(d, c) => s.push_str(&format!("  shl {}, {}\n", d.to_string(), c.to_string())),
            X86Instr::Shr(d, c) => s.push_str(&format!("  shr {}, {}\n", d.to_string(), c.to_string())),            X86Instr::Movsx(d, src) => s.push_str(&format!("  movsx {}, {}\\n", d.to_string(), src.to_string())),        }
    }
    s
}
