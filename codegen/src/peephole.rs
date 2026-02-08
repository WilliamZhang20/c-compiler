// Peephole optimization pass for assembly-level improvements
use crate::x86::{X86Instr, X86Operand, X86Reg};

/// apply_peephole performs pattern-based optimizations on generated assembly
pub fn apply_peephole(instructions: &mut Vec<X86Instr>) {
    let mut i = 0;
    while i < instructions.len() {
        let removed = try_optimize_at(instructions, i);
        if !removed {
            i += 1;
        }
    }
}

fn try_optimize_at(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    // Pattern 1: mov reg, reg -> remove (no-op)
    if let Some(X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Reg(r2))) = instructions.get(i) {
        if matches!((r1, r2), 
            (X86Reg::Rax, X86Reg::Rax) | (X86Reg::Rcx, X86Reg::Rcx) | 
            (X86Reg::Rdx, X86Reg::Rdx) | (X86Reg::Rbx, X86Reg::Rbx) |
            (X86Reg::Rsi, X86Reg::Rsi) | (X86Reg::Rdi, X86Reg::Rdi) |
            (X86Reg::R8, X86Reg::R8) | (X86Reg::R9, X86Reg::R9) |
            (X86Reg::R10, X86Reg::R10) | (X86Reg::R11, X86Reg::R11) |
            (X86Reg::R12, X86Reg::R12) | (X86Reg::R13, X86Reg::R13) |
            (X86Reg::R14, X86Reg::R14) | (X86Reg::R15, X86Reg::R15)
        ) {
            instructions.remove(i);
            return true;
        }
    }

    // Pattern 2: mov reg, X; mov Y, reg -> mov Y, X (if reg not used after)
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(temp_reg), src),
            X86Instr::Mov(dest, X86Operand::Reg(temp_reg2))
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(temp_reg) == std::mem::discriminant(temp_reg2) {
                if !is_reg_used_after(instructions, i + 2, temp_reg) {
                    // Replace both with direct move
                    instructions[i] = X86Instr::Mov(dest.clone(), src.clone());
                    instructions.remove(i + 1);
                    return true;
                }
            }
        }
    }

    // Pattern 3: add/sub with 0 -> remove
    if let Some(X86Instr::Add(_, X86Operand::Imm(0))) | Some(X86Instr::Sub(_, X86Operand::Imm(0))) = instructions.get(i) {
        instructions.remove(i);
        return true;
    }

    // Pattern 4: imul reg, 1 -> remove
    if let Some(X86Instr::Imul(_, X86Operand::Imm(1))) = instructions.get(i) {
        instructions.remove(i);
        return true;
    }

    // Pattern 5: mov reg, imm; add reg, X -> lea reg, [X + imm] (for small constants)
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Imm(offset)),
            X86Instr::Add(X86Operand::Reg(r2), X86Operand::Reg(r3))
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(r1) == std::mem::discriminant(r2) {
                if *offset >= -128 && *offset <= 127 {
                    // Use LEA for address calculation
                    instructions[i] = X86Instr::Lea(
                        X86Operand::Reg(r1.clone()),
                        X86Operand::Mem(r3.clone(), *offset as i32)
                    );
                    instructions.remove(i + 1);
                    return true;
                }
            }
        }
    }

    false
}

fn is_reg_used_after(instructions: &[X86Instr], start: usize, reg: &X86Reg) -> bool {
    for inst in instructions.iter().skip(start) {
        // Simple check - if we see the register used, return true
        // This is conservative but safe
        match inst {
            X86Instr::Mov(dest, src) => {
                if matches_reg(src, reg) || matches_reg(dest, reg) {
                    return true;
                }
            }
            X86Instr::Add(dest, src) => {
                if matches_reg(src, reg) || matches_reg(dest, reg) {
                    return true;
                }
            }
            X86Instr::Sub(dest, src) => {
                if matches_reg(src, reg) || matches_reg(dest, reg) {
                    return true;
                }
            }
            X86Instr::Imul(_, src) | X86Instr::Cmp(_, src) => {
                if matches_reg(src, reg) {
                    return true;
                }
            }
            X86Instr::Label(_) => return false, // Don't optimize across labels
            _ => {}
        }
    }
    false
}

fn matches_reg(operand: &X86Operand, reg: &X86Reg) -> bool {
    match operand {
        X86Operand::Reg(r) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        X86Operand::Mem(r, _) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        _ => false,
    }
}
