// X86-64 register and instruction definitions

#[derive(Debug, Clone, PartialEq)]
pub enum X86Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Eax, Ecx, Edx, Ebx, Ebp, Esi, Edi, Esp, // 32-bit registers
    R8d, R9d, R10d, R11d, R12d, R13d, R14d, R15d, // 32-bit extended
    Al, Cl, // 8-bit low bytes of rax, rcx
    Ax, Cx, // 16-bit registers
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
            Self::Ax => "ax", Self::Cx => "cx",
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
    WordMem(X86Reg, i32), // [reg + offset] - WORD PTR (16-bit)
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
            Self::WordMem(r, offset) => format!("WORD PTR [{}{:+}]", r.to_str(), offset),
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

/// emit_asm converts X86 instructions to Intel syntax assembly
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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Register naming ────────────────────────────────────────
    #[test]
    fn reg_names_64bit() {
        assert_eq!(X86Reg::Rax.to_str(), "rax");
        assert_eq!(X86Reg::Rdi.to_str(), "rdi");
        assert_eq!(X86Reg::R15.to_str(), "r15");
    }

    #[test]
    fn reg_names_32bit() {
        assert_eq!(X86Reg::Eax.to_str(), "eax");
        assert_eq!(X86Reg::R8d.to_str(), "r8d");
    }

    #[test]
    fn reg_names_8bit() {
        assert_eq!(X86Reg::Al.to_str(), "al");
        assert_eq!(X86Reg::Cl.to_str(), "cl");
    }

    #[test]
    fn reg_names_xmm() {
        assert_eq!(X86Reg::Xmm0.to_str(), "xmm0");
        assert_eq!(X86Reg::Xmm7.to_str(), "xmm7");
    }

    // ─── Operand formatting ─────────────────────────────────────
    #[test]
    fn operand_reg() {
        let op = X86Operand::Reg(X86Reg::Rax);
        assert_eq!(op.to_string(), "rax");
    }

    #[test]
    fn operand_imm() {
        assert_eq!(X86Operand::Imm(42).to_string(), "42");
        assert_eq!(X86Operand::Imm(-1).to_string(), "-1");
    }

    #[test]
    fn operand_mem_positive_offset() {
        let op = X86Operand::Mem(X86Reg::Rbp, -8);
        assert_eq!(op.to_string(), "QWORD PTR [rbp-8]");
    }

    #[test]
    fn operand_dword_mem() {
        let op = X86Operand::DwordMem(X86Reg::Rbp, -4);
        assert_eq!(op.to_string(), "DWORD PTR [rbp-4]");
    }

    #[test]
    fn operand_word_mem() {
        let op = X86Operand::WordMem(X86Reg::Rbp, -6);
        assert_eq!(op.to_string(), "WORD PTR [rbp-6]");
    }

    #[test]
    fn operand_byte_mem() {
        let op = X86Operand::ByteMem(X86Reg::Rsp, 0);
        assert_eq!(op.to_string(), "BYTE PTR [rsp+0]");
    }

    #[test]
    fn operand_label() {
        let op = X86Operand::Label(".L1".to_string());
        assert_eq!(op.to_string(), ".L1");
    }

    #[test]
    fn operand_global_mem() {
        let op = X86Operand::GlobalMem("my_global".to_string());
        assert_eq!(op.to_string(), "DWORD PTR my_global[rip]");
    }

    #[test]
    fn operand_rip_rel() {
        let op = X86Operand::RipRelLabel("str_0".to_string());
        assert_eq!(op.to_string(), "str_0[rip]");
    }

    #[test]
    fn operand_float_mem() {
        let op = X86Operand::FloatMem(X86Reg::Rbp, -16);
        assert_eq!(op.to_string(), "DWORD PTR [rbp-16]");
    }

    // ─── emit_asm ───────────────────────────────────────────────
    #[test]
    fn emit_mov() {
        let instrs = vec![X86Instr::Mov(
            X86Operand::Reg(X86Reg::Rax),
            X86Operand::Imm(42),
        )];
        assert_eq!(emit_asm(&instrs), "  mov rax, 42\n");
    }

    #[test]
    fn emit_add_sub() {
        let instrs = vec![
            X86Instr::Add(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(1)),
            X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(2)),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("add rax, 1"));
        assert!(asm.contains("sub rax, 2"));
    }

    #[test]
    fn emit_label() {
        let instrs = vec![X86Instr::Label(".Lentry".to_string())];
        assert_eq!(emit_asm(&instrs), ".Lentry:\n");
    }

    #[test]
    fn emit_push_pop() {
        let instrs = vec![
            X86Instr::Push(X86Reg::Rbp),
            X86Instr::Pop(X86Reg::Rbp),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("push rbp"));
        assert!(asm.contains("pop rbp"));
    }

    #[test]
    fn emit_ret() {
        let instrs = vec![X86Instr::Ret];
        assert_eq!(emit_asm(&instrs), "  ret\n");
    }

    #[test]
    fn emit_call() {
        let instrs = vec![X86Instr::Call("printf".to_string())];
        assert_eq!(emit_asm(&instrs), "  call printf\n");
    }

    #[test]
    fn emit_jmp_jcc() {
        let instrs = vec![
            X86Instr::Jmp(".L1".to_string()),
            X86Instr::Jcc("e".to_string(), ".L2".to_string()),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("jmp .L1"));
        assert!(asm.contains("je .L2"));
    }

    #[test]
    fn emit_cmp_set() {
        let instrs = vec![
            X86Instr::Cmp(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)),
            X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("cmp rax, 0"));
        assert!(asm.contains("sete al"));
    }

    #[test]
    fn emit_shift_ops() {
        let instrs = vec![
            X86Instr::Shl(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(3)),
            X86Instr::Shr(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(1)),
            X86Instr::Sar(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Cl)),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("shl rax, 3"));
        assert!(asm.contains("shr rax, 1"));
        assert!(asm.contains("sar rax, cl"));
    }

    #[test]
    fn emit_float_instrs() {
        let instrs = vec![
            X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)),
            X86Instr::Addss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)),
        ];
        let asm = emit_asm(&instrs);
        assert!(asm.contains("movss xmm0, xmm1"));
        assert!(asm.contains("addss xmm0, xmm1"));
    }

    #[test]
    fn emit_raw() {
        let instrs = vec![X86Instr::Raw("nop".to_string())];
        assert_eq!(emit_asm(&instrs), "  nop\n");
    }

    #[test]
    fn emit_neg() {
        let instrs = vec![X86Instr::Neg(X86Operand::Reg(X86Reg::Rax))];
        assert_eq!(emit_asm(&instrs), "  neg rax\n");
    }

    #[test]
    fn emit_leave() {
        assert_eq!(emit_asm(&[X86Instr::Leave]), "  leave\n");
    }

    #[test]
    fn emit_empty() {
        assert_eq!(emit_asm(&[]), "");
    }
}
