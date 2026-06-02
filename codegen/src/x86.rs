// X86-64 register and instruction definitions
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum X86Reg {
    Rax, Rcx, Rdx, Rbx, Rsp, Rbp, Rsi, Rdi,
    R8, R9, R10, R11, R12, R13, R14, R15,
    Eax, Ecx, Edx, Ebx, Ebp, Esi, Edi, Esp, // 32-bit registers
    R8d, R9d, R10d, R11d, R12d, R13d, R14d, R15d, // 32-bit extended
    Al, Cl, // 8-bit low bytes of rax, rcx
    Ax, Cx, // 16-bit registers
    Xmm0, Xmm1, Xmm2, Xmm3, Xmm4, Xmm5, Xmm6, Xmm7, // SSE float registers
    Xmm8, Xmm9, Xmm10, Xmm11, Xmm12, Xmm13, Xmm14, Xmm15, // SSE extended
    Ymm0, Ymm1, Ymm2, Ymm3, Ymm4, Ymm5, Ymm6, Ymm7, // AVX 256-bit registers
    Ymm8, Ymm9, Ymm10, Ymm11, Ymm12, Ymm13, Ymm14, Ymm15, // AVX extended
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
            Self::Xmm8 => "xmm8", Self::Xmm9 => "xmm9", Self::Xmm10 => "xmm10", Self::Xmm11 => "xmm11",
            Self::Xmm12 => "xmm12", Self::Xmm13 => "xmm13", Self::Xmm14 => "xmm14", Self::Xmm15 => "xmm15",
            Self::Ymm0 => "ymm0", Self::Ymm1 => "ymm1", Self::Ymm2 => "ymm2", Self::Ymm3 => "ymm3",
            Self::Ymm4 => "ymm4", Self::Ymm5 => "ymm5", Self::Ymm6 => "ymm6", Self::Ymm7 => "ymm7",
            Self::Ymm8 => "ymm8", Self::Ymm9 => "ymm9", Self::Ymm10 => "ymm10", Self::Ymm11 => "ymm11",
            Self::Ymm12 => "ymm12", Self::Ymm13 => "ymm13", Self::Ymm14 => "ymm14", Self::Ymm15 => "ymm15",
        }
    }

    /// Convert a 64-bit GPR to its 32-bit variant. Non-GPR registers are returned unchanged.
    pub fn to_32bit(&self) -> Self {
        match self {
            Self::Rax | Self::Eax => Self::Eax,
            Self::Rcx | Self::Ecx => Self::Ecx,
            Self::Rdx | Self::Edx => Self::Edx,
            Self::Rbx | Self::Ebx => Self::Ebx,
            Self::Rsp | Self::Esp => Self::Esp,
            Self::Rbp | Self::Ebp => Self::Ebp,
            Self::Rsi | Self::Esi => Self::Esi,
            Self::Rdi | Self::Edi => Self::Edi,
            Self::R8 | Self::R8d => Self::R8d,
            Self::R9 | Self::R9d => Self::R9d,
            Self::R10 | Self::R10d => Self::R10d,
            Self::R11 | Self::R11d => Self::R11d,
            Self::R12 | Self::R12d => Self::R12d,
            Self::R13 | Self::R13d => Self::R13d,
            Self::R14 | Self::R14d => Self::R14d,
            Self::R15 | Self::R15d => Self::R15d,
            other => other.clone(),
        }
    }

    /// Return a stable numeric ID for the physical register,
    /// collapsing aliases (rax/eax/ax/al → 0, xmm0/ymm0 → 16, etc.).
    pub fn physical_id(&self) -> u8 {
        match self {
            Self::Rax | Self::Eax | Self::Ax | Self::Al => 0,
            Self::Rcx | Self::Ecx | Self::Cx | Self::Cl => 1,
            Self::Rdx | Self::Edx => 2,
            Self::Rbx | Self::Ebx => 3,
            Self::Rsp | Self::Esp => 4,
            Self::Rbp | Self::Ebp => 5,
            Self::Rsi | Self::Esi => 6,
            Self::Rdi | Self::Edi => 7,
            Self::R8 | Self::R8d => 8,
            Self::R9 | Self::R9d => 9,
            Self::R10 | Self::R10d => 10,
            Self::R11 | Self::R11d => 11,
            Self::R12 | Self::R12d => 12,
            Self::R13 | Self::R13d => 13,
            Self::R14 | Self::R14d => 14,
            Self::R15 | Self::R15d => 15,
            Self::Xmm0 | Self::Ymm0 => 16,
            Self::Xmm1 | Self::Ymm1 => 17,
            Self::Xmm2 | Self::Ymm2 => 18,
            Self::Xmm3 | Self::Ymm3 => 19,
            Self::Xmm4 | Self::Ymm4 => 20,
            Self::Xmm5 | Self::Ymm5 => 21,
            Self::Xmm6 | Self::Ymm6 => 22,
            Self::Xmm7 | Self::Ymm7 => 23,
            Self::Xmm8 | Self::Ymm8 => 24,
            Self::Xmm9 | Self::Ymm9 => 25,
            Self::Xmm10 | Self::Ymm10 => 26,
            Self::Xmm11 | Self::Ymm11 => 27,
            Self::Xmm12 | Self::Ymm12 => 28,
            Self::Xmm13 | Self::Ymm13 => 29,
            Self::Xmm14 | Self::Ymm14 => 30,
            Self::Xmm15 | Self::Ymm15 => 31,
        }
    }

    /// Returns true if two registers share the same physical register.
    pub fn same_physical(&self, other: &Self) -> bool {
        self.physical_id() == other.physical_id()
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
    FloatMem(X86Reg, i32), // [reg + offset] for float ops - DWORD PTR (32-bit single)
    DoubleMem(X86Reg, i32), // [reg + offset] for double ops - QWORD PTR (64-bit double)
    GlobalQwordMem(String), // RIP-relative global: QWORD PTR label[rip]
    XmmwordMem(X86Reg, i32), // [reg + offset] - 128-bit (XMMWORD PTR)
    YmmwordMem(X86Reg, i32), // [reg + offset] - 256-bit (YMMWORD PTR)
}

impl fmt::Display for X86Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reg(r) => f.write_str(r.to_str()),
            Self::Mem(r, offset) => fmt_mem(f, "QWORD PTR", r, *offset),
            Self::DwordMem(r, offset) => fmt_mem(f, "DWORD PTR", r, *offset),
            Self::WordMem(r, offset) => fmt_mem(f, "WORD PTR", r, *offset),
            Self::ByteMem(r, offset) => fmt_mem(f, "BYTE PTR", r, *offset),
            Self::Imm(i) => write!(f, "{}", i),
            Self::Label(s) => f.write_str(s),
            Self::GlobalMem(name) => write!(f, "DWORD PTR {}[rip]", name),
            Self::RipRelLabel(name) => write!(f, "{}[rip]", name),
            Self::FloatMem(r, offset) => fmt_mem(f, "DWORD PTR", r, *offset),
            Self::DoubleMem(r, offset) => fmt_mem(f, "QWORD PTR", r, *offset),
            Self::GlobalQwordMem(name) => write!(f, "QWORD PTR {}[rip]", name),
            Self::XmmwordMem(r, offset) => fmt_mem(f, "XMMWORD PTR", r, *offset),
            Self::YmmwordMem(r, offset) => fmt_mem(f, "YMMWORD PTR", r, *offset),
        }
    }
}

/// Format a memory operand, omitting the offset when it's zero.
fn fmt_mem(f: &mut fmt::Formatter<'_>, size: &str, reg: &X86Reg, offset: i32) -> fmt::Result {
    if offset == 0 {
        write!(f, "{} [{}]", size, reg.to_str())
    } else {
        write!(f, "{} [{}{:+}]", size, reg.to_str(), offset)
    }
}

impl X86Operand {
    /// Returns true if this operand references the given register,
    /// either as a direct register or as a memory base.
    pub fn references_reg(&self, reg: &X86Reg) -> bool {
        match self {
            Self::Reg(r) => r.same_physical(reg),
            Self::Mem(r, _) | Self::DwordMem(r, _) | Self::WordMem(r, _) |
            Self::ByteMem(r, _) | Self::FloatMem(r, _) | Self::DoubleMem(r, _) |
            Self::XmmwordMem(r, _) | Self::YmmwordMem(r, _) => r.same_physical(reg),
            _ => false,
        }
    }

    /// Returns true if this operand is a direct register matching the given physical register.
    pub fn is_direct_reg(&self, reg: &X86Reg) -> bool {
        matches!(self, Self::Reg(r) if r.same_physical(reg))
    }

    /// Returns true if this operand uses the register as a memory base address.
    pub fn has_base_reg(&self, reg: &X86Reg) -> bool {
        match self {
            Self::Mem(r, _) | Self::DwordMem(r, _) | Self::WordMem(r, _) |
            Self::ByteMem(r, _) | Self::FloatMem(r, _) | Self::DoubleMem(r, _) |
            Self::XmmwordMem(r, _) | Self::YmmwordMem(r, _) => r.same_physical(reg),
            _ => false,
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
    // Double-precision (64-bit) float instructions
    Movsd(X86Operand, X86Operand),
    Addsd(X86Operand, X86Operand),
    Subsd(X86Operand, X86Operand),
    Mulsd(X86Operand, X86Operand),
    Divsd(X86Operand, X86Operand),
    Ucomisd(X86Operand, X86Operand),
    Cvtsi2sd(X86Operand, X86Operand),
    Cvttsd2si(X86Operand, X86Operand),
    Xorpd(X86Operand, X86Operand),
    Cvtss2sd(X86Operand, X86Operand), // float -> double
    Cvtsd2ss(X86Operand, X86Operand), // double -> float
    Neg(X86Operand), // TWO'S COMPLEMENT NEGATION
    // Packed SSE instructions (128-bit, 4x float)
    Movaps(X86Operand, X86Operand),   // Move aligned packed single-precision
    Movups(X86Operand, X86Operand),   // Move unaligned packed single-precision
    Addps(X86Operand, X86Operand),    // Add packed single-precision (4x)
    Subps(X86Operand, X86Operand),    // Subtract packed single-precision (4x)
    Mulps(X86Operand, X86Operand),    // Multiply packed single-precision (4x)
    Divps(X86Operand, X86Operand),    // Divide packed single-precision (4x)
    // Packed SSE2 instructions (128-bit, 4x int32)
    Movdqa(X86Operand, X86Operand),   // Move aligned packed integers
    Movdqu(X86Operand, X86Operand),   // Move unaligned packed integers
    Paddd(X86Operand, X86Operand),    // Add packed 32-bit integers
    Psubd(X86Operand, X86Operand),    // Subtract packed 32-bit integers
    Pmulld(X86Operand, X86Operand),   // Multiply packed 32-bit integers (SSE4.1)
    Pand(X86Operand, X86Operand),     // Packed AND 32-bit integers
    Pandn(X86Operand, X86Operand),    // Packed AND-NOT 32-bit integers
    Por(X86Operand, X86Operand),      // Packed OR 32-bit integers
    Pcmpgtd(X86Operand, X86Operand),    // Compare packed 32-bit integers (gt → all-ones)
    // AVX instructions (256-bit, 8x float)
    Vmovaps(X86Operand, X86Operand),  // AVX move aligned packed single
    Vmovups(X86Operand, X86Operand),  // AVX move unaligned packed single
    Vaddps(X86Operand, X86Operand, X86Operand),   // AVX add packed single (3-operand)
    Vsubps(X86Operand, X86Operand, X86Operand),   // AVX subtract packed single
    Vmulps(X86Operand, X86Operand, X86Operand),   // AVX multiply packed single
    Vdivps(X86Operand, X86Operand, X86Operand),   // AVX divide packed single
    // AVX2 instructions (256-bit, 8x int32)
    Vmovdqa(X86Operand, X86Operand),  // AVX move aligned packed integers
    Vmovdqu(X86Operand, X86Operand),  // AVX move unaligned packed integers
    Vpaddd(X86Operand, X86Operand, X86Operand),   // AVX2 add packed 32-bit integers
    Vpsubd(X86Operand, X86Operand, X86Operand),   // AVX2 subtract packed 32-bit integers
    Vpmulld(X86Operand, X86Operand, X86Operand),  // AVX2 multiply packed 32-bit integers
    Vpandd(X86Operand, X86Operand, X86Operand),   // AVX2 packed AND
    Vpandnd(X86Operand, X86Operand, X86Operand),  // AVX2 packed AND-NOT
    Vpord(X86Operand, X86Operand, X86Operand),    // AVX2 packed OR
    Vpcmpgtd(X86Operand, X86Operand, X86Operand), // AVX2 compare dwords (s1 > s2)
    Vxorps(X86Operand, X86Operand, X86Operand),   // AVX XOR packed
    Vandps(X86Operand, X86Operand, X86Operand),   // AVX bitwise AND (float bits)
    Vandnps(X86Operand, X86Operand, X86Operand),    // AVX bitwise AND-NOT
    Vorps(X86Operand, X86Operand, X86Operand),      // AVX bitwise OR
    Vzeroupper,                                     // Clear upper bits of YMM registers
    // SIMD utility instructions
    Pshufd(X86Operand, X86Operand, u8),             // Shuffle packed doublewords
    Movd(X86Operand, X86Operand),                   // Move doubleword (GPR <-> XMM)
    Pxor(X86Operand, X86Operand),                   // Packed XOR integers
    // AVX2 utility instructions
    Vextracti128(X86Operand, X86Operand, u8),       // Extract 128-bit from 256-bit
    Vpxor(X86Operand, X86Operand, X86Operand),      // AVX packed XOR
    Vpbroadcastd(X86Operand, X86Operand),            // AVX2 broadcast dword to all lanes
    /// dest = gather from [R10 + index*4] with mask (R10 set by caller).
    Vpgatherdd(X86Operand, X86Operand, X86Operand),
    /// scatter value to [R10 + index*4] with mask (R10 set by caller).
    Vpscatterdd(X86Operand, X86Operand, X86Operand),
    Raw(String), // Raw assembly string (for inline asm)
}

impl X86Instr {
    /// Returns true if this instruction reads the given physical register.
    /// Conservatively matches the existing peephole liveness semantics.
    pub fn reads_phys_reg(&self, reg: &X86Reg) -> bool {
        match self {
            // Mov-like: reads src; reads mem-dest base (store needs base address)
            X86Instr::Mov(dest, src) | X86Instr::Movss(dest, src) |
            X86Instr::Movsd(dest, src) => {
                src.references_reg(reg) || dest.has_base_reg(reg)
            }
            // Write-only dest, read src
            X86Instr::Lea(_, src) | X86Instr::Movsx(_, src) |
            X86Instr::Movzx(_, src) => src.references_reg(reg),
            // Shuffle/extract: write-only dest, read src
            X86Instr::Pshufd(_, src, _) | X86Instr::Vextracti128(_, src, _) |
            X86Instr::Vpbroadcastd(_, src) => src.references_reg(reg),
            // ALU read-modify-write, compares, conversions, packed SSE/AVX 2-operand:
            // conservatively treat both operands as read
            X86Instr::Add(d, s) | X86Instr::Sub(d, s) | X86Instr::And(d, s) |
            X86Instr::Or(d, s) | X86Instr::Xor(d, s) | X86Instr::Imul(d, s) |
            X86Instr::Cmp(d, s) | X86Instr::Test(d, s) |
            X86Instr::Shl(d, s) | X86Instr::Shr(d, s) | X86Instr::Sar(d, s) |
            X86Instr::Addss(d, s) | X86Instr::Subss(d, s) | X86Instr::Mulss(d, s) |
            X86Instr::Divss(d, s) | X86Instr::Ucomiss(d, s) | X86Instr::Cvtsi2ss(d, s) |
            X86Instr::Cvttss2si(d, s) | X86Instr::Xorps(d, s) |
            X86Instr::Addsd(d, s) | X86Instr::Subsd(d, s) | X86Instr::Mulsd(d, s) |
            X86Instr::Divsd(d, s) | X86Instr::Ucomisd(d, s) | X86Instr::Cvtsi2sd(d, s) |
            X86Instr::Cvttsd2si(d, s) | X86Instr::Xorpd(d, s) |
            X86Instr::Cvtss2sd(d, s) | X86Instr::Cvtsd2ss(d, s) |
            X86Instr::Movaps(d, s) | X86Instr::Movups(d, s) |
            X86Instr::Addps(d, s) | X86Instr::Subps(d, s) | X86Instr::Mulps(d, s) |
            X86Instr::Divps(d, s) | X86Instr::Movdqa(d, s) | X86Instr::Movdqu(d, s) |
            X86Instr::Paddd(d, s) | X86Instr::Psubd(d, s) | X86Instr::Pmulld(d, s) |
            X86Instr::Pand(d, s) | X86Instr::Pandn(d, s) | X86Instr::Pcmpgtd(d, s) |
            X86Instr::Por(d, s) |
            X86Instr::Pxor(d, s) | X86Instr::Movd(d, s) |
            X86Instr::Vmovaps(d, s) | X86Instr::Vmovups(d, s) |
            X86Instr::Vmovdqa(d, s) | X86Instr::Vmovdqu(d, s) => {
                d.references_reg(reg) || s.references_reg(reg)
            }
            // AVX 3-operand: conservatively treat all three as read
            X86Instr::Vaddps(d, s1, s2) | X86Instr::Vsubps(d, s1, s2) |
            X86Instr::Vmulps(d, s1, s2) | X86Instr::Vdivps(d, s1, s2) |
            X86Instr::Vpaddd(d, s1, s2) | X86Instr::Vpsubd(d, s1, s2) |
            X86Instr::Vpmulld(d, s1, s2) | X86Instr::Vpandd(d, s1, s2) |
            X86Instr::Vpandnd(d, s1, s2) | X86Instr::Vpcmpgtd(d, s1, s2) |
            X86Instr::Vpord(d, s1, s2) | X86Instr::Vxorps(d, s1, s2) |
            X86Instr::Vandps(d, s1, s2) | X86Instr::Vandnps(d, s1, s2) |
            X86Instr::Vorps(d, s1, s2) |
            X86Instr::Vpxor(d, s1, s2) |
            X86Instr::Vpgatherdd(d, s1, s2) | X86Instr::Vpscatterdd(d, s1, s2) => {
                d.references_reg(reg) || s1.references_reg(reg) || s2.references_reg(reg)
            }
            // Single-operand read-modify-write
            X86Instr::Neg(op) | X86Instr::Not(op) => op.references_reg(reg),
            // Idiv: reads operand + implicit rax, rdx
            X86Instr::Idiv(op) => {
                op.references_reg(reg) || reg.physical_id() == 0 || reg.physical_id() == 2
            }
            // Set: partial byte write, no read
            X86Instr::Set(_, _) => false,
            // Push reads the register
            X86Instr::Push(r) => r.same_physical(reg),
            // Pop overwrites only
            X86Instr::Pop(_) => false,
            // CallIndirect reads the operand
            X86Instr::CallIndirect(op) => op.references_reg(reg),
            // Cqto/Cdq: reads rax
            X86Instr::Cqto | X86Instr::Cdq => reg.physical_id() == 0,
            // Leave (mov rsp,rbp; pop rbp): reads rbp
            X86Instr::Leave => reg.physical_id() == 5,
            // Control flow and zero-operand
            X86Instr::Label(_) | X86Instr::Jmp(_) | X86Instr::Jcc(_, _) |
            X86Instr::Call(_) | X86Instr::Ret | X86Instr::Vzeroupper => false,
            // Raw: conservative
            X86Instr::Raw(_) => true,
        }
    }

    /// Returns true if this instruction definitively overwrites (kills) the register.
    pub fn writes_phys_reg(&self, reg: &X86Reg) -> bool {
        match self {
            // Mov-like: kills dest when it's a direct register
            X86Instr::Mov(dest, _) | X86Instr::Lea(dest, _) |
            X86Instr::Movsx(dest, _) | X86Instr::Movzx(dest, _) |
            X86Instr::Movss(dest, _) | X86Instr::Movsd(dest, _) => {
                dest.is_direct_reg(reg)
            }
            // Shuffle/extract/broadcast: kills dest register
            X86Instr::Pshufd(dest, _, _) | X86Instr::Vextracti128(dest, _, _) |
            X86Instr::Vpbroadcastd(dest, _) => dest.is_direct_reg(reg),
            // AVX 3-operand: kills dest register
            X86Instr::Vaddps(dest, _, _) | X86Instr::Vsubps(dest, _, _) |
            X86Instr::Vmulps(dest, _, _) | X86Instr::Vdivps(dest, _, _) |
            X86Instr::Vpaddd(dest, _, _) | X86Instr::Vpsubd(dest, _, _) |
            X86Instr::Vpmulld(dest, _, _) | X86Instr::Vxorps(dest, _, _) |
            X86Instr::Vpxor(dest, _, _) | X86Instr::Vpgatherdd(dest, _, _) => dest.is_direct_reg(reg),
            // Idiv: kills rax (quotient) and rdx (remainder)
            X86Instr::Idiv(_) => reg.physical_id() == 0 || reg.physical_id() == 2,
            // Set: partial write — handled by partially_writes_phys_reg.
            X86Instr::Set(_, _) => false,
            // Pop overwrites register
            X86Instr::Pop(r) => r.same_physical(reg),
            // Cqto/Cdq: kills rdx
            X86Instr::Cqto | X86Instr::Cdq => reg.physical_id() == 2,
            // Leave: kills rsp (and rbp, but read comes first)
            X86Instr::Leave => reg.physical_id() == 4,
            _ => false,
        }
    }

    /// Returns true if this instruction partially writes the register without
    /// killing the full value.  For example, `setl al` modifies the low byte of
    /// rax but leaves the upper 56 bits unchanged.
    ///
    /// Partial writes are *not* full kills — liveness must keep treating the
    /// previous full-width value as live — but they do "touch" the register,
    /// meaning intervening-instruction analyses (e.g., copy forwarding) must
    /// stop scanning past them.
    pub fn partially_writes_phys_reg(&self, reg: &X86Reg) -> bool {
        match self {
            X86Instr::Set(_, dest) => dest.is_direct_reg(reg),
            _ => false,
        }
    }

    /// Returns true if this instruction reads, writes, or partially writes the
    /// physical register.  This is the broadest "does it touch?" query — useful
    /// for peephole copy-forwarding guards that must stop at *any* interaction.
    pub fn touches_phys_reg(&self, reg: &X86Reg) -> bool {
        self.reads_phys_reg(reg)
            || self.writes_phys_reg(reg)
            || self.partially_writes_phys_reg(reg)
    }

    /// Returns true if this instruction is a basic-block boundary.
    pub fn is_block_boundary(&self) -> bool {
        matches!(self,
            X86Instr::Jmp(_) | X86Instr::Jcc(_, _) | X86Instr::Label(_) |
            X86Instr::Ret | X86Instr::Call(_) | X86Instr::CallIndirect(_)
        )
    }
}

/// emit_asm converts X86 instructions to Intel syntax assembly
pub fn emit_asm(instructions: &[X86Instr]) -> String {
    use fmt::Write;
    let mut s = String::new();
    for instr in instructions {
        match instr {
            X86Instr::Label(l) => { let _ = write!(s, "{}:\n", l); }
            X86Instr::Mov(d, src) => {
                // 32-bit writes to eax zero-extend rax; gas rejects `mov rax, eax`.
                if matches!(d, X86Operand::Reg(X86Reg::Rax))
                    && matches!(src, X86Operand::Reg(X86Reg::Eax))
                {
                    continue;
                }
                let _ = write!(s, "  mov {}, {}\n", d, src);
            }
            X86Instr::Add(d, src) => { let _ = write!(s, "  add {}, {}\n", d, src); }
            X86Instr::Sub(d, src) => { let _ = write!(s, "  sub {}, {}\n", d, src); }
            X86Instr::Neg(d) => { let _ = write!(s, "  neg {}\n", d); }
            X86Instr::Imul(d, src) => { let _ = write!(s, "  imul {}, {}\n", d, src); }
            X86Instr::Idiv(src) => { let _ = write!(s, "  idiv {}\n", src); }
            X86Instr::Cmp(l, r) => { let _ = write!(s, "  cmp {}, {}\n", l, r); }
            X86Instr::Test(l, r) => { let _ = write!(s, "  test {}, {}\n", l, r); }
            X86Instr::Set(c, d) => { let _ = write!(s, "  set{} {}\n", c, d); }
            X86Instr::Jmp(l) => { let _ = write!(s, "  jmp {}\n", l); }
            X86Instr::Jcc(c, l) => { let _ = write!(s, "  j{} {}\n", c, l); }
            X86Instr::Push(r) => { let _ = write!(s, "  push {}\n", r.to_str()); }
            X86Instr::Pop(r) => { let _ = write!(s, "  pop {}\n", r.to_str()); }
            X86Instr::Call(l) => { let _ = write!(s, "  call {}\n", l); }
            X86Instr::CallIndirect(op) => { let _ = write!(s, "  call {}\n", op); }
            X86Instr::Ret => s.push_str("  ret\n"),
            X86Instr::Leave => s.push_str("  leave\n"),
            X86Instr::Cqto => s.push_str("  cqo\n"),
            X86Instr::Cdq => s.push_str("  cdq\n"),
            X86Instr::Xor(d, s_op) => { let _ = write!(s, "  xor {}, {}\n", d, s_op); }
            X86Instr::Lea(d, s_op) => { let _ = write!(s, "  lea {}, {}\n", d, s_op); }
            X86Instr::And(d, s_op) => { let _ = write!(s, "  and {}, {}\n", d, s_op); }
            X86Instr::Or(d, s_op) => { let _ = write!(s, "  or {}, {}\n", d, s_op); }
            X86Instr::Not(d) => { let _ = write!(s, "  not {}\n", d); }
            X86Instr::Shl(d, c) => { let _ = write!(s, "  shl {}, {}\n", d, c); }
            X86Instr::Shr(d, c) => { let _ = write!(s, "  shr {}, {}\n", d, c); }
            X86Instr::Sar(d, c) => { let _ = write!(s, "  sar {}, {}\n", d, c); }
            X86Instr::Movsx(d, src) => { let _ = write!(s, "  movsx {}, {}\n", d, src); }
            X86Instr::Movzx(d, src) => { let _ = write!(s, "  movzx {}, {}\n", d, src); }
            // Float instructions
            X86Instr::Movss(d, src) => { let _ = write!(s, "  movss {}, {}\n", d, src); }
            X86Instr::Addss(d, src) => { let _ = write!(s, "  addss {}, {}\n", d, src); }
            X86Instr::Subss(d, src) => { let _ = write!(s, "  subss {}, {}\n", d, src); }
            X86Instr::Mulss(d, src) => { let _ = write!(s, "  mulss {}, {}\n", d, src); }
            X86Instr::Divss(d, src) => { let _ = write!(s, "  divss {}, {}\n", d, src); }
            X86Instr::Ucomiss(l, r) => { let _ = write!(s, "  ucomiss {}, {}\n", l, r); }
            X86Instr::Cvtsi2ss(d, src) => { let _ = write!(s, "  cvtsi2ss {}, {}\n", d, src); }
            X86Instr::Cvttss2si(d, src) => { let _ = write!(s, "  cvttss2si {}, {}\n", d, src); }
            X86Instr::Xorps(d, src) => { let _ = write!(s, "  xorps {}, {}\n", d, src); }
            // Double-precision float instructions
            X86Instr::Movsd(d, src) => { let _ = write!(s, "  movsd {}, {}\n", d, src); }
            X86Instr::Addsd(d, src) => { let _ = write!(s, "  addsd {}, {}\n", d, src); }
            X86Instr::Subsd(d, src) => { let _ = write!(s, "  subsd {}, {}\n", d, src); }
            X86Instr::Mulsd(d, src) => { let _ = write!(s, "  mulsd {}, {}\n", d, src); }
            X86Instr::Divsd(d, src) => { let _ = write!(s, "  divsd {}, {}\n", d, src); }
            X86Instr::Ucomisd(l, r) => { let _ = write!(s, "  ucomisd {}, {}\n", l, r); }
            X86Instr::Cvtsi2sd(d, src) => { let _ = write!(s, "  cvtsi2sd {}, {}\n", d, src); }
            X86Instr::Cvttsd2si(d, src) => { let _ = write!(s, "  cvttsd2si {}, {}\n", d, src); }
            X86Instr::Xorpd(d, src) => { let _ = write!(s, "  xorpd {}, {}\n", d, src); }
            X86Instr::Cvtss2sd(d, src) => { let _ = write!(s, "  cvtss2sd {}, {}\n", d, src); }
            X86Instr::Cvtsd2ss(d, src) => { let _ = write!(s, "  cvtsd2ss {}, {}\n", d, src); }
            // Packed SSE
            X86Instr::Movaps(d, src) => { let _ = write!(s, "  movaps {}, {}\n", d, src); }
            X86Instr::Movups(d, src) => { let _ = write!(s, "  movups {}, {}\n", d, src); }
            X86Instr::Addps(d, src) => { let _ = write!(s, "  addps {}, {}\n", d, src); }
            X86Instr::Subps(d, src) => { let _ = write!(s, "  subps {}, {}\n", d, src); }
            X86Instr::Mulps(d, src) => { let _ = write!(s, "  mulps {}, {}\n", d, src); }
            X86Instr::Divps(d, src) => { let _ = write!(s, "  divps {}, {}\n", d, src); }
            // Packed SSE2 integer
            X86Instr::Movdqa(d, src) => { let _ = write!(s, "  movdqa {}, {}\n", d, src); }
            X86Instr::Movdqu(d, src) => { let _ = write!(s, "  movdqu {}, {}\n", d, src); }
            X86Instr::Paddd(d, src) => { let _ = write!(s, "  paddd {}, {}\n", d, src); }
            X86Instr::Psubd(d, src) => { let _ = write!(s, "  psubd {}, {}\n", d, src); }
            X86Instr::Pmulld(d, src) => { let _ = write!(s, "  pmulld {}, {}\n", d, src); }
            X86Instr::Pand(d, src) => { let _ = write!(s, "  pand {}, {}\n", d, src); }
            X86Instr::Pandn(d, src) => { let _ = write!(s, "  pandn {}, {}\n", d, src); }
            X86Instr::Pcmpgtd(d, src) => { let _ = write!(s, "  pcmpgtd {}, {}\n", d, src); }
            X86Instr::Por(d, src) => { let _ = write!(s, "  por {}, {}\n", d, src); }
            // AVX
            X86Instr::Vmovaps(d, src) => { let _ = write!(s, "  vmovaps {}, {}\n", d, src); }
            X86Instr::Vmovups(d, src) => { let _ = write!(s, "  vmovups {}, {}\n", d, src); }
            X86Instr::Vaddps(d, s1, s2) => { let _ = write!(s, "  vaddps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vsubps(d, s1, s2) => { let _ = write!(s, "  vsubps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vmulps(d, s1, s2) => { let _ = write!(s, "  vmulps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vdivps(d, s1, s2) => { let _ = write!(s, "  vdivps {}, {}, {}\n", d, s1, s2); }
            // AVX2
            X86Instr::Vmovdqa(d, src) => { let _ = write!(s, "  vmovdqa {}, {}\n", d, src); }
            X86Instr::Vmovdqu(d, src) => { let _ = write!(s, "  vmovdqu {}, {}\n", d, src); }
            X86Instr::Vpaddd(d, s1, s2) => { let _ = write!(s, "  vpaddd {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpsubd(d, s1, s2) => { let _ = write!(s, "  vpsubd {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpmulld(d, s1, s2) => { let _ = write!(s, "  vpmulld {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpandd(d, s1, s2) => { let _ = write!(s, "  vpandd {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpandnd(d, s1, s2) => { let _ = write!(s, "  vpandnd {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpcmpgtd(d, s1, s2) => { let _ = write!(s, "  vpcmpgtd {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpord(d, s1, s2) => { let _ = write!(s, "  vpord {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vxorps(d, s1, s2) => { let _ = write!(s, "  vxorps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vandps(d, s1, s2) => { let _ = write!(s, "  vandps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vandnps(d, s1, s2) => { let _ = write!(s, "  vandnps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vorps(d, s1, s2) => { let _ = write!(s, "  vorps {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vzeroupper => { s.push_str("  vzeroupper\n"); }
            // SIMD utility
            X86Instr::Pshufd(d, src, imm) => { let _ = write!(s, "  pshufd {}, {}, {}\n", d, src, imm); }
            X86Instr::Movd(d, src) => { let _ = write!(s, "  movd {}, {}\n", d, src); }
            X86Instr::Pxor(d, src) => { let _ = write!(s, "  pxor {}, {}\n", d, src); }
            X86Instr::Vextracti128(d, src, imm) => { let _ = write!(s, "  vextracti128 {}, {}, {}\n", d, src, imm); }
            X86Instr::Vpxor(d, s1, s2) => { let _ = write!(s, "  vpxor {}, {}, {}\n", d, s1, s2); }
            X86Instr::Vpbroadcastd(d, src) => { let _ = write!(s, "  vpbroadcastd {}, {}\n", d, src); }
            X86Instr::Vpgatherdd(d, idx, mask) => {
                let _ = write!(s, "  vpgatherdd {}, DWORD PTR [r10 + {}*4], {}\n", d, idx, mask);
            }
            X86Instr::Vpscatterdd(idx, val, mask) => {
                let _ = write!(s, "  vpscatterdd DWORD PTR [r10 + {}*4], {}, {}\n", idx, val, mask);
            }
            X86Instr::Raw(asm_str) => { let _ = write!(s, "  {}\n", asm_str); }
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
        assert_eq!(op.to_string(), "BYTE PTR [rsp]");
        let op2 = X86Operand::ByteMem(X86Reg::Rsp, 4);
        assert_eq!(op2.to_string(), "BYTE PTR [rsp+4]");
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
