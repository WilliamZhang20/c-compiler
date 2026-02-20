// X86-64 register and instruction definitions

#[derive(Debug, Clone, PartialEq)]
pub enum X86Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Eax, Ecx, Edx, Ebx, Ebp, Esi, Edi, Esp, // 32-bit registers
    R8d, R9d, R10d, R11d, R12d, R13d, R14d, R15d, // 32-bit extended
    Al, Cl, // 8-bit low bytes of rax, rcx
    Xmm0, Xmm1, Xmm2, Xmm3, Xmm4, Xmm5, Xmm6, Xmm7, // SSE float registers
}

impl X86Reg {
    pub fn to_str(&self) -> &str {
        match self {
            Self::Rax => "rax", Self::Rcx => "rcx", Self::Rdx => "rdx", Self::Rbx => "rbx",
            Self::Rsp => "rsp", Self::Rbp => "rbp", Self::Rsi => "rsi", Self::Rdi => "rdi",
            Self::R8 => "r8", Self::R9 => "r9", Self::R10 => "r10", Self::R11 => "r11",
            Self::R12 => "r12", Self::R13 => "r13", Self::R14 => "r14", Self::R15 => "r15",
            Self::Eax => "eax", Self::Ecx => "ecx", Self::Edx => "edx", Self::Ebx => "ebx",
            Self::Ebp => "ebp", Self::Esi => "esi", Self::Edi => "edi", Self::Esp => "esp",
            Self::R8d => "r8d", Self::R9d => "r9d", Self::R10d => "r10d", Self::R11d => "r11d",
            Self::R12d => "r12d", Self::R13d => "r13d", Self::R14d => "r14d", Self::R15d => "r15d",
            Self::Al => "al", Self::Cl => "cl",
            Self::Xmm0 => "xmm0", Self::Xmm1 => "xmm1", Self::Xmm2 => "xmm2", Self::Xmm3 => "xmm3",
            Self::Xmm4 => "xmm4", Self::Xmm5 => "xmm5", Self::Xmm6 => "xmm6", Self::Xmm7 => "xmm7",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum X86Operand {
    Reg(X86Reg),
    Mem(X86Reg, i32), // [reg + offset] - QWORD PTR
    DwordMem(X86Reg, i32), // [reg + offset] - DWORD PTR (32-bit)
    ByteMem(X86Reg, i32), // [reg + offset] - BYTE PTR (8-bit)
    Imm(i64),
    Label(String),
    GlobalMem(String), // RIP-relative global: label[rip]
    RipRelLabel(String), // For LEA: label[rip]
    FloatMem(X86Reg, i32), // [reg + offset] for float ops (no PTR directive for SSE)
}

impl X86Operand {
    pub fn to_string(&self) -> String {
        match self {
            Self::Reg(r) => r.to_str().to_string(),
            Self::Mem(r, offset) => format!("QWORD PTR [{}{:+}]", r.to_str(), offset),
            Self::DwordMem(r, offset) => format!("DWORD PTR [{}{:+}]", r.to_str(), offset),
            Self::ByteMem(r, offset) => format!("BYTE PTR [{}{:+}]", r.to_str(), offset),
            Self::Imm(i) => i.to_string(),
            Self::Label(s) => s.clone(), // Just emit the label as-is (for LEA)
            Self::GlobalMem(name) => format!("DWORD PTR {}[rip]", name), // RIP-relative 32-bit int access
            Self::RipRelLabel(name) => format!("{}[rip]", name), // RIP-relative label for LEA
            Self::FloatMem(r, offset) => format!("DWORD PTR [{}{:+}]", r.to_str(), offset),
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
    Test(X86Operand, X86Operand),
    Set(String, X86Operand),
    Jmp(String),
    Jcc(String, String),
    Push(X86Reg),
    Pop(X86Reg),
    Call(String),
    CallIndirect(X86Operand),
    Ret,
    Leave,
    Label(String),
    Cqto,
    Cdq,
    Xor(X86Operand, X86Operand),
    Lea(X86Operand, X86Operand),
    And(X86Operand, X86Operand),
    Or(X86Operand, X86Operand),
    Not(X86Operand),
    Shl(X86Operand, X86Operand),
    Shr(X86Operand, X86Operand),
    Sar(X86Operand, X86Operand), // Arithmetic (signed) right shift
    Movsx(X86Operand, X86Operand), // Sign-extend smaller value into larger register
    Movzx(X86Operand, X86Operand), // Zero-extend
    // Float instructions
    Movss(X86Operand, X86Operand), // Move scalar single-precision float
    Addss(X86Operand, X86Operand), // Add scalar single-precision float
    Subss(X86Operand, X86Operand), // Subtract scalar single-precision float
    Mulss(X86Operand, X86Operand), // Multiply scalar single-precision float
    Divss(X86Operand, X86Operand), // Divide scalar single-precision float
    Ucomiss(X86Operand, X86Operand), // Compare scalar single-precision float
    Cvtsi2ss(X86Operand, X86Operand), // Convert int to float
    Cvttss2si(X86Operand, X86Operand), // Convert float to int (truncate)
    Xorps(X86Operand, X86Operand), // XOR packed single-precision (for negation)
    Neg(X86Operand), // TWO'S COMPLEMENT NEGATION
    Raw(String), // Raw assembly string (for inline asm)
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
            X86Instr::Neg(d) => s.push_str(&format!("  neg {}\n", d.to_string())),
            X86Instr::Imul(d, src) => s.push_str(&format!("  imul {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Idiv(src) => s.push_str(&format!("  idiv {}\n", src.to_string())),
            X86Instr::Cmp(l, r) => s.push_str(&format!("  cmp {}, {}\n", l.to_string(), r.to_string())),
            X86Instr::Test(l, r) => s.push_str(&format!("  test {}, {}\n", l.to_string(), r.to_string())),
            X86Instr::Set(c, d) => s.push_str(&format!("  set{} {}\n", c, d.to_string())),
            X86Instr::Jmp(l) => s.push_str(&format!("  jmp {}\n", l)),
            X86Instr::Jcc(c, l) => s.push_str(&format!("  j{} {}\n", c, l)),
            X86Instr::Push(r) => s.push_str(&format!("  push {}\n", r.to_str())),
            X86Instr::Pop(r) => s.push_str(&format!("  pop {}\n", r.to_str())),
            X86Instr::Call(l) => s.push_str(&format!("  call {}\n", l)),            X86Instr::CallIndirect(op) => s.push_str(&format!("  call {}\n", op.to_string())),            X86Instr::Ret => s.push_str("  ret\n"),
            X86Instr::Leave => s.push_str("  leave\n"),
            X86Instr::Cqto => s.push_str("  cqo\n"),
            X86Instr::Cdq => s.push_str("  cdq\n"),
            X86Instr::Xor(d, s_op) => s.push_str(&format!("  xor {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::Lea(d, s_op) => s.push_str(&format!("  lea {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::And(d, s_op) => s.push_str(&format!("  and {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::Or(d, s_op) => s.push_str(&format!("  or {}, {}\n", d.to_string(), s_op.to_string())),
            X86Instr::Not(d) => s.push_str(&format!("  not {}\n", d.to_string())),
            X86Instr::Shl(d, c) => s.push_str(&format!("  shl {}, {}\n", d.to_string(), c.to_string())),
            X86Instr::Shr(d, c) => s.push_str(&format!("  shr {}, {}\n", d.to_string(), c.to_string())),
            X86Instr::Sar(d, c) => s.push_str(&format!("  sar {}, {}\n", d.to_string(), c.to_string())),
            X86Instr::Movsx(d, src) => s.push_str(&format!("  movsx {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Movzx(d, src) => s.push_str(&format!("  movzx {}, {}\n", d.to_string(), src.to_string())),
            // Float instructions
            X86Instr::Movss(d, src) => s.push_str(&format!("  movss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Addss(d, src) => s.push_str(&format!("  addss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Subss(d, src) => s.push_str(&format!("  subss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Mulss(d, src) => s.push_str(&format!("  mulss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Divss(d, src) => s.push_str(&format!("  divss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Ucomiss(l, r) => s.push_str(&format!("  ucomiss {}, {}\n", l.to_string(), r.to_string())),
            X86Instr::Cvtsi2ss(d, src) => s.push_str(&format!("  cvtsi2ss {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Cvttss2si(d, src) => s.push_str(&format!("  cvttss2si {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Xorps(d, src) => s.push_str(&format!("  xorps {}, {}\n", d.to_string(), src.to_string())),
            X86Instr::Raw(asm_str) => s.push_str(&format!("  {}\n", asm_str)),
        }
    }
    s
}
