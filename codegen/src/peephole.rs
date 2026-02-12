// Peephole optimization pass for assembly-level improvements
use crate::x86::{X86Instr, X86Operand, X86Reg};
use std::collections::HashMap;

/// apply_peephole performs pattern-based optimizations on generated assembly
pub fn apply_peephole(instructions: &mut Vec<X86Instr>) {
    // First pass: eliminate jump chains
    eliminate_jump_chains(instructions);
    
    // Second pass: other peephole optimizations
    let mut i = 0;
    while i < instructions.len() {
        let removed = try_optimize_at(instructions, i);
        if !removed {
            i += 1;
        }
    }
}

/// Eliminate jump-to-jump chains: if label A jumps to label B which immediately jumps to label C,
/// redirect all jumps to A to go directly to C
fn eliminate_jump_chains(instructions: &mut Vec<X86Instr>) {
    // Build a map of label -> target if the label immediately jumps
    let mut jump_targets: HashMap<String, String> = HashMap::new();
    
    let mut i = 0;
    while i < instructions.len() {
        if let X86Instr::Label(label) = &instructions[i] {
            // Check if next instruction is a jump
            if i + 1 < instructions.len() {
                if let X86Instr::Jmp(target) = &instructions[i + 1] {
                    jump_targets.insert(label.clone(), target.clone());
                }
            }
        }
        i += 1;
    }
    
    // Resolve transitive jumps: if A -> B and B -> C, then A -> C
    for _ in 0..10 {  // Max 10 iterations to prevent infinite loops
        let mut changed = false;
        let entries: Vec<(String, String)> = jump_targets.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        for (label, target) in entries {
            if let Some(new_target) = jump_targets.get(&target).cloned() {
                if new_target != label {  // Avoid self-loops
                    jump_targets.insert(label, new_target);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    
    // Now redirect all jumps using the map
    for inst in instructions.iter_mut() {
        match inst {
            X86Instr::Jmp(target) => {
                if let Some(new_target) = jump_targets.get(target) {
                    *target = new_target.clone();
                }
            }
            X86Instr::Jcc(_, target) => {
                if let Some(new_target) = jump_targets.get(target) {
                    *target = new_target.clone();
                }
            }
            _ => {}
        }
    }
    
    // Finally, remove useless label+jmp pairs that are now dead
    let mut i = 0;
    while i + 1 < instructions.len() {
        if let (X86Instr::Label(_), X86Instr::Jmp(_)) = (&instructions[i], &instructions[i + 1]) {
            // Check if this label is still referenced
            let label_name = if let X86Instr::Label(name) = &instructions[i] {
                name.clone()
            } else {
                unreachable!()
            };
            
            let is_referenced = instructions.iter().enumerate().any(|(idx, inst)| {
                if idx == i { return false; }  // Don't count self
                match inst {
                    X86Instr::Jmp(t) | X86Instr::Jcc(_, t) => t == &label_name,
                    _ => false,
                }
            });
            
            if !is_referenced {
                // Check if reachable by fallthrough from previous instruction
                // Unconditional jumps and returns execute control flow change, others fall through
                let fallthrough_reachable = if i == 0 {
                    true
                } else {
                    match &instructions[i-1] {
                        X86Instr::Jmp(_) | X86Instr::Ret => false,
                        _ => true,
                    }
                };

                if !fallthrough_reachable {
                    // unreachable - remove both label and jump
                    instructions.remove(i);
                    instructions.remove(i); 
                    continue;
                } else {
                     // reachable - remove only the label, keep the jump
                     instructions.remove(i);
                     // The Jump is now at i. We must continue to verify next instructions.
                     // Since we removed 1 instruction, next iteration at i will check Jmp + next.
                     continue;
                }
            }
        }
        i += 1;
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
                    // Check if this would create a memory-to-memory move (illegal in x86)
                    let is_src_mem = matches!(src, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::FloatMem(..) | X86Operand::GlobalMem(..));
                    let is_dest_mem = matches!(dest, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::FloatMem(..) | X86Operand::GlobalMem(..));
                    
                    if !is_src_mem || !is_dest_mem {
                        // Safe to optimize: at least one operand is not memory
                        instructions[i] = X86Instr::Mov(dest.clone(), src.clone());
                        instructions.remove(i + 1);
                        return true;
                    }
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
    
    // Pattern 6: mov eax, [mem]; movsx rax, eax; mov reg, rax -> movsx reg, [mem]
    // TEMPORARILY DISABLED - causing segfault in matmul benchmark
    // The movsx from memory should be correct, but needs investigation
    /*
    if i + 2 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), src @ X86Operand::DwordMem(..)),
            X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)),
            X86Instr::Mov(dest, X86Operand::Reg(X86Reg::Rax))
        ) = (&instructions[i], &instructions[i + 1], &instructions[i + 2]) {
            // Combine into a single movsx from memory to destination
            instructions[i] = X86Instr::Movsx(dest.clone(), src.clone());
            instructions.remove(i + 1);
            instructions.remove(i + 1);  // After first remove, second is now at i+1
            return true;
        }
    }
    */
    
    // Pattern 7: DISABLED - was causing operand type mismatches
    // Redundant load elimination removed - it was forwarding registers across
    // size-changing operations causing "mov 64-bit-reg, 32-bit-reg" mismatches
    
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
            // Conservative: assume used for any control flow or unknown instruction
            X86Instr::Label(_) | X86Instr::Jmp(_) | X86Instr::Jcc(_, _) | 
            X86Instr::Call(_) | X86Instr::CallIndirect(_) | X86Instr::Ret => return true,
            
            // For other instructions, check operands if possible, otherwise assume usage
            X86Instr::Movsx(dest, src) | X86Instr::Movzx(dest, src) |
            X86Instr::Movss(dest, src) | X86Instr::Addss(dest, src) |
            X86Instr::Subss(dest, src) | X86Instr::Mulss(dest, src) |
            X86Instr::Divss(dest, src) | X86Instr::Ucomiss(dest, src) |
            X86Instr::Xorps(dest, src) | X86Instr::Cvtsi2ss(dest, src) |
            X86Instr::Cvttss2si(dest, src) | X86Instr::And(dest, src) |
            X86Instr::Or(dest, src) | X86Instr::Xor(dest, src) |
            X86Instr::Lea(dest, src) | X86Instr::Shl(dest, src) |
            X86Instr::Shr(dest, src) | X86Instr::Test(dest, src) => {
                if matches_reg(dest, reg) || matches_reg(src, reg) {
                    return true;
                }
            }
            _ => return true,
        }
    }
    false
}

fn matches_reg(operand: &X86Operand, reg: &X86Reg) -> bool {
    match operand {
        X86Operand::Reg(r) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        X86Operand::Mem(r, _) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        X86Operand::DwordMem(r, _) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        X86Operand::FloatMem(r, _) => std::mem::discriminant(r) == std::mem::discriminant(reg),
        _ => false,
    }
}
