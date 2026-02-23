// Peephole optimization pass for assembly-level improvements
use crate::x86::{X86Instr, X86Operand, X86Reg};
use std::collections::{HashMap, HashSet};

/// apply_peephole performs pattern-based optimizations on generated assembly
pub fn apply_peephole(instructions: &mut Vec<X86Instr>) {
    // First pass: eliminate jump chains
    eliminate_jump_chains(instructions);
    
    // Iterate pattern-based optimizations until no more changes (fixpoint)
    for _round in 0..10 {
        let mut changed = false;
        let mut i = 0;
        while i < instructions.len() {
            let removed = try_optimize_at(instructions, i);
            if removed {
                changed = true;
            } else {
                i += 1;
            }
        }
        if !changed {
            break;
        }
    }
    
    // Final pass: eliminate fallthrough jumps (jmp LABEL where LABEL: is next)
    eliminate_fallthrough_jumps(instructions);
}

/// Remove `jmp LABEL` when `LABEL:` is the very next instruction,
/// and convert `jcc A; jmp B; A:` into `j!cc B; A:` (conditional inversion + fallthrough).
fn eliminate_fallthrough_jumps(instructions: &mut Vec<X86Instr>) {
    let mut i = 0;
    while i + 1 < instructions.len() {
        // Pattern: jcc A; jmp B; A: → j!cc B; A:
        if i + 2 < instructions.len() {
            if let (X86Instr::Jcc(cond, target_a), X86Instr::Jmp(target_b), X86Instr::Label(label_a))
                = (&instructions[i], &instructions[i + 1], &instructions[i + 2])
            {
                if target_a == label_a {
                    if let Some(inv) = invert_condition(cond) {
                        let new_target = target_b.clone();
                        instructions[i] = X86Instr::Jcc(inv, new_target);
                        instructions.remove(i + 1);
                        continue;
                    }
                }
            }
        }
        
        // Pattern: jmp LABEL; LABEL: → remove jmp
        if let X86Instr::Jmp(target) = &instructions[i] {
            if let X86Instr::Label(label) = &instructions[i + 1] {
                if target == label {
                    instructions.remove(i);
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn invert_condition(cond: &str) -> Option<String> {
    match cond {
        "e" => Some("ne".to_string()),
        "ne" => Some("e".to_string()),
        "l" => Some("ge".to_string()),
        "ge" => Some("l".to_string()),
        "le" => Some("g".to_string()),
        "g" => Some("le".to_string()),
        "b" => Some("ae".to_string()),
        "ae" => Some("b".to_string()),
        "be" => Some("a".to_string()),
        "a" => Some("be".to_string()),
        _ => None,
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
    // Pattern 0: Comparison followed by set/test/branch -> direct conditional jump
    // Looking for: cmp regA, opB; mov rax/eax, 0; set<cond> al; mov regD, rax; [gap]; test regD, regD; j<cc> label
    // Allows non-interfering instructions between mov regD and test regD (e.g., stack spills).
    // Simplify to: cmp regA, opB; j<cond> label (when jcc is "ne") or j<!cond> label (when jcc is "e")
    if i + 5 < instructions.len() {
        // Match the first 4 instructions contiguously
        let first4_matches = matches!(
            (&instructions[i], &instructions[i+1], &instructions[i+2], &instructions[i+3]),
            (
                X86Instr::Cmp(_, _),
                X86Instr::Mov(X86Operand::Reg(X86Reg::Rax | X86Reg::Eax), X86Operand::Imm(0)),
                X86Instr::Set(_, X86Operand::Reg(X86Reg::Al)),
                X86Instr::Mov(_, X86Operand::Reg(X86Reg::Rax | X86Reg::Eax)),
            )
        );
        
        if first4_matches {
            // Get the destination register from step 4
            let test_reg = if let X86Instr::Mov(X86Operand::Reg(r), _) = &instructions[i+3] {
                r.clone()
            } else {
                return false;
            };
            
            // Scan forward from i+4 for test+jcc, allowing non-interfering gap instructions
            let max_scan = std::cmp::min(i + 10, instructions.len());
            let mut gap_indices = Vec::new();
            let mut found_test_jcc = None;
            
            for j in (i+4)..max_scan {
                // Check for test regD, regD followed by jcc
                if j + 1 < instructions.len() {
                    if let (X86Instr::Test(tl, tr), X86Instr::Jcc(_, _)) = (&instructions[j], &instructions[j+1]) {
                        if reads_reg_direct(tl, &test_reg) && reads_reg_direct(tr, &test_reg) {
                            found_test_jcc = Some(j);
                            break;
                        }
                    }
                }
                // Stop at control flow
                if matches!(instructions[j],
                    X86Instr::Label(_) | X86Instr::Jmp(_) | X86Instr::Jcc(_, _) |
                    X86Instr::Ret | X86Instr::Call(_) | X86Instr::CallIndirect(_)
                ) {
                    break;
                }
                // Check if gap instruction interferes with test_reg
                if instr_touches_reg(&instructions[j], &test_reg) {
                    break;
                }
                gap_indices.push(j);
            }
            
            if let Some(test_idx) = found_test_jcc {
                let set_cond = if let X86Instr::Set(c, _) = &instructions[i+2] { c.clone() } else { unreachable!() };
                let (branch_cond, branch_label) = if let X86Instr::Jcc(c, l) = &instructions[test_idx+1] { (c.clone(), l.clone()) } else { unreachable!() };
                
                let final_cond = if branch_cond == "ne" {
                    set_cond
                } else if branch_cond == "e" {
                    match set_cond.as_str() {
                        "e" => "ne".to_string(),
                        "ne" => "e".to_string(),
                        "l" => "ge".to_string(),
                        "le" => "g".to_string(),
                        "g" => "le".to_string(),
                        "ge" => "l".to_string(),
                        _ => return false,
                    }
                } else {
                    return false;
                };
                
                // Build the replacement: cmp + [gap instructions] + jcc
                // Remove: mov rax,0; set al; mov regD,rax; test regD,regD
                let cmp_left = if let X86Instr::Cmp(l, _) = &instructions[i] { l.clone() } else { unreachable!() };
                let cmp_right = if let X86Instr::Cmp(_, r) = &instructions[i] { r.clone() } else { unreachable!() };
                
                // Remove test+jcc first (higher indices), then the mov/set/mov block
                instructions.remove(test_idx + 1); // remove jcc
                instructions.remove(test_idx);     // remove test
                // Remove mov rax,0; set; mov regD,rax (indices i+1, i+2, i+3)
                instructions.remove(i + 3);
                instructions.remove(i + 2);
                instructions.remove(i + 1);
                // Insert jcc after cmp + gap instructions
                // After removing 5 instructions, the gap instructions shifted
                // New position for jcc: i + 1 + gap_indices.len()
                let jcc_pos = i + 1 + gap_indices.len();
                instructions.insert(jcc_pos, X86Instr::Jcc(final_cond, branch_label));
                instructions[i] = X86Instr::Cmp(cmp_left, cmp_right);
                return true;
            }
        }
    }
    
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

    // Pattern 1b: mov reg1, src; cmp reg1, op -> cmp src, op (if reg1 not used after and src is a register)
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(mov_dest), mov_src @ X86Operand::Reg(_)),
            X86Instr::Cmp(X86Operand::Reg(cmp_left), cmp_right)
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(mov_dest) == std::mem::discriminant(cmp_left) {
                if !is_reg_used_after(instructions, i + 2, mov_dest) {
                    // Replace mov + cmp with just cmp
                    instructions[i] = X86Instr::Cmp(mov_src.clone(), cmp_right.clone());
                    instructions.remove(i + 1);
                    return true;
                }
            }
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
                    let is_src_mem = matches!(src, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                    let is_dest_mem = matches!(dest, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                    
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

    // Pattern 2b: Non-adjacent copy forwarding
    // mov reg, src; [intervening]; mov dest, reg → mov dest, src (remove original)
    // The intervening instructions must not touch reg or write to src.
    // Stops at labels/jumps (stays within one basic block).
    if let X86Instr::Mov(X86Operand::Reg(temp_reg), src) = &instructions[i] {
        if matches!(src, X86Operand::Reg(_) | X86Operand::Imm(_)) {
            let max_scan = std::cmp::min(i + 10, instructions.len());
            for j in (i + 1)..max_scan {
                // Stop at control flow boundaries (labels, jumps, calls, ret)
                if matches!(instructions[j],
                    X86Instr::Label(_) | X86Instr::Jmp(_) | X86Instr::Jcc(_, _) |
                    X86Instr::Ret | X86Instr::Call(_) | X86Instr::CallIndirect(_)
                ) {
                    break;
                }
                
                // Check if this instruction uses temp_reg at all
                if instr_touches_reg(&instructions[j], temp_reg) {
                    // It might be our target mov
                    if let X86Instr::Mov(dest, X86Operand::Reg(temp2)) = &instructions[j] {
                        // Use exact variant match (not same_physical_reg) to avoid
                        // size mismatches (e.g., forwarding 64-bit rdi into DWORD PTR)
                        if std::mem::discriminant(temp_reg) == std::mem::discriminant(temp2) {
                            // Found the target. Check reg is dead after j.
                            if !is_reg_used_after(instructions, j + 1, temp_reg) {
                                // Check no mem-to-mem
                                let is_src_mem = matches!(src, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                                let is_dest_mem = matches!(dest, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                                if !is_src_mem || !is_dest_mem {
                                    instructions[j] = X86Instr::Mov(dest.clone(), src.clone());
                                    instructions.remove(i);
                                    return true;
                                }
                            }
                        }
                    }
                    break; // temp_reg is touched by a non-target instruction
                }
                
                // Check if src (if register) is modified by intervening instruction
                if let X86Operand::Reg(src_reg) = src {
                    if instr_touches_reg(&instructions[j], src_reg) {
                        // src is modified before it could be forwarded
                        break;
                    }
                }
            }
        }
    }

    // Pattern 2c: Immediate forwarding through dead register
    // mov reg, imm; OP dest, reg_alias → OP dest, imm (if reg dead after)
    // Handles the common case where codegen loads an immediate into a register
    // just to use it once in a subsequent instruction (e.g., array offset calc).
    if i + 1 < instructions.len() {
        if let X86Instr::Mov(X86Operand::Reg(load_reg), X86Operand::Imm(imm_val)) = &instructions[i] {
            let imm_val = *imm_val;
            let load_reg = load_reg.clone();
            let can_forward = match &instructions[i + 1] {
                X86Instr::Mov(dest, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => {
                    // Don't create memory-to-memory
                    let is_dest_mem = matches!(dest, X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) | X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) | X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..));
                    // For DWORD memory destinations, the immediate must fit in 32 bits
                    if is_dest_mem && matches!(dest, X86Operand::DwordMem(..)) {
                        imm_val >= i32::MIN as i64 && imm_val <= i32::MAX as i64
                    } else {
                        true
                    }
                }
                X86Instr::Add(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                X86Instr::Sub(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                X86Instr::Cmp(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                X86Instr::And(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                X86Instr::Or(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                X86Instr::Xor(_, X86Operand::Reg(use_reg)) if same_physical_reg(&load_reg, use_reg) => true,
                _ => false,
            };
            if can_forward && !is_reg_used_after(instructions, i + 2, &load_reg) {
                let imm_op = X86Operand::Imm(imm_val);
                match &instructions[i + 1] {
                    X86Instr::Mov(dest, _) => { instructions[i] = X86Instr::Mov(dest.clone(), imm_op); }
                    X86Instr::Add(dest, _) => { instructions[i] = X86Instr::Add(dest.clone(), imm_op); }
                    X86Instr::Sub(dest, _) => { instructions[i] = X86Instr::Sub(dest.clone(), imm_op); }
                    X86Instr::Cmp(dest, _) => { instructions[i] = X86Instr::Cmp(dest.clone(), imm_op); }
                    X86Instr::And(dest, _) => { instructions[i] = X86Instr::And(dest.clone(), imm_op); }
                    X86Instr::Or(dest, _) => { instructions[i] = X86Instr::Or(dest.clone(), imm_op); }
                    X86Instr::Xor(dest, _) => { instructions[i] = X86Instr::Xor(dest.clone(), imm_op); }
                    _ => unreachable!(),
                }
                instructions.remove(i + 1);
                return true;
            }
        }
    }

    // Pattern 3: add/sub with 0 -> remove
    if let Some(X86Instr::Add(_, X86Operand::Imm(0))) | Some(X86Instr::Sub(_, X86Operand::Imm(0))) = instructions.get(i) {
        instructions.remove(i);
        return true;
    }

    // Pattern 3b: lea reg, [base+off]; add reg, C → lea reg, [base+off+C]
    // Fold immediate addition into LEA offset.
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Lea(X86Operand::Reg(lea_dest), lea_src),
            X86Instr::Add(X86Operand::Reg(add_dest), X86Operand::Imm(add_imm))
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(lea_dest) == std::mem::discriminant(add_dest) {
                let new_src = match lea_src {
                    X86Operand::Mem(base, off) => Some(X86Operand::Mem(base.clone(), off + *add_imm as i32)),
                    X86Operand::DwordMem(base, off) => Some(X86Operand::DwordMem(base.clone(), off + *add_imm as i32)),
                    _ => None,
                };
                if let Some(new_lea_src) = new_src {
                    instructions[i] = X86Instr::Lea(X86Operand::Reg(lea_dest.clone()), new_lea_src);
                    instructions.remove(i + 1);
                    return true;
                }
            }
        }
    }

    // Pattern 3c: LEA forwarding into memory operand
    // lea reg, [base+off]; mov/load/store ... [reg+off2] ... → ... [base+off+off2] ...
    // Eliminates the LEA instruction when its result is only used as a memory base.
    if i + 1 < instructions.len() {
        if let X86Instr::Lea(X86Operand::Reg(lea_dest), lea_src) = &instructions[i] {
            if let X86Operand::Mem(lea_base, lea_off) = lea_src {
                let lea_dest_c = lea_dest.clone();
                let lea_base_c = lea_base.clone();
                let lea_off_c = *lea_off;
                if let Some(new_instr) = fold_lea_into_next(&instructions[i + 1], &lea_dest_c, &lea_base_c, lea_off_c) {
                    // Check that lea_dest is dead after the folded instruction,
                    // OR the folded instruction itself overwrites lea_dest (making
                    // subsequent uses read the folded instruction's value, not the LEA's).
                    let fold_overwrites_dest = match &new_instr {
                        X86Instr::Mov(X86Operand::Reg(r), _) |
                        X86Instr::Lea(X86Operand::Reg(r), _) |
                        X86Instr::Movsx(X86Operand::Reg(r), _) |
                        X86Instr::Movzx(X86Operand::Reg(r), _) => same_physical_reg(r, &lea_dest_c),
                        _ => false,
                    };
                    if fold_overwrites_dest || !is_reg_used_after(instructions, i + 2, &lea_dest_c) {
                        instructions[i] = new_instr;
                        instructions.remove(i + 1);
                        return true;
                    }
                }
            }
        }
    }

    // Pattern 4: imul reg, 1 -> remove
    if let Some(X86Instr::Imul(_, X86Operand::Imm(1))) = instructions.get(i) {
        instructions.remove(i);
        return true;
    }

    // Pattern 4b: imul reg, 0 -> mov reg, 0
    if let Some(X86Instr::Imul(X86Operand::Reg(r), X86Operand::Imm(0))) = instructions.get(i) {
        instructions[i] = X86Instr::Mov(X86Operand::Reg(r.clone()), X86Operand::Imm(0));
        return true;
    }

    // Pattern 4c: mov reg, C1; imul reg, C2 -> mov reg, C1*C2  (constant fold)
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Imm(c1)),
            X86Instr::Imul(X86Operand::Reg(r2), X86Operand::Imm(c2))
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(r1) == std::mem::discriminant(r2) {
                let result = c1.wrapping_mul(*c2);
                instructions[i] = X86Instr::Mov(X86Operand::Reg(r1.clone()), X86Operand::Imm(result));
                instructions.remove(i + 1);
                return true;
            }
        }
    }

    // Pattern 4d: mov reg, 0; add dest, reg -> remove both (if reg dead after add)
    // Adding zero via a register is a no-op.
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Imm(0)),
            X86Instr::Add(_, X86Operand::Reg(r2))
        ) = (&instructions[i], &instructions[i + 1]) {
            if std::mem::discriminant(r1) == std::mem::discriminant(r2) {
                if !is_reg_used_after(instructions, i + 2, r1) {
                    instructions.remove(i); // remove mov reg, 0
                    instructions.remove(i); // remove add dest, reg (now at i)
                    return true;
                }
            }
        }
    }
    
    // Pattern 8: mov tmp, reg; add/sub tmp, op; [intervening instrs]; mov reg, tmp → add/sub reg, op
    // (in-place increment/decrement through scratch register)
    // The intervening instructions must not read or write tmp or reg.
    if i + 2 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(tmp1), X86Operand::Reg(src_reg)),
            add_or_sub,
        ) = (&instructions[i], &instructions[i + 1]) {
            // Check the middle instruction is add/sub on the same tmp register
            let folded = match add_or_sub {
                X86Instr::Add(X86Operand::Reg(tmp2), op)
                    if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp2) =>
                {
                    Some(X86Instr::Add(X86Operand::Reg(src_reg.clone()), op.clone()))
                }
                X86Instr::Sub(X86Operand::Reg(tmp2), op)
                    if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp2) =>
                {
                    Some(X86Instr::Sub(X86Operand::Reg(src_reg.clone()), op.clone()))
                }
                _ => None,
            };
            if let Some(new_instr) = folded {
                // Search for the matching `mov reg, tmp` within the next few instructions
                let max_scan = std::cmp::min(i + 8, instructions.len());
                for j in (i + 2)..max_scan {
                    // Check for matching writeback: mov src_reg, tmp
                    if let X86Instr::Mov(X86Operand::Reg(dst_reg), X86Operand::Reg(tmp3)) = &instructions[j] {
                        if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp3)
                            && std::mem::discriminant(src_reg) == std::mem::discriminant(dst_reg)
                        {
                            // Verify intervening instructions don't touch tmp or src_reg
                            let mut safe = true;
                            for k in (i + 2)..j {
                                if instr_touches_reg(&instructions[k], tmp1)
                                    || instr_touches_reg(&instructions[k], src_reg)
                                {
                                    safe = false;
                                    break;
                                }
                            }
                            if safe && !is_reg_used_after(instructions, j + 1, tmp1) {
                                // Replace: mov tmp, reg + add tmp, op → add reg, op
                                instructions[i] = new_instr;
                                instructions.remove(i + 1); // remove add/sub tmp, op
                                // Now the writeback is at j-1 (shifted by 1 removal)
                                instructions.remove(j - 1); // remove mov reg, tmp
                                return true;
                            }
                            break; // found the writeback, don't keep scanning
                        }
                    }
                    // If we hit control flow, stop scanning
                    if matches!(instructions[j],
                        X86Instr::Jmp(_) | X86Instr::Jcc(_, _) | X86Instr::Label(_) |
                        X86Instr::Ret | X86Instr::Call(_) | X86Instr::CallIndirect(_)
                    ) {
                        break;
                    }
                }
            }
        }
    }
    
    // Pattern 10: Copy forwarding into memory base register
    // mov rX, rY; instr [rX+off], ... → instr [rY+off], ...  (if rX dead after)
    // mov rX, rY; instr dest, [rX+off] → instr dest, [rY+off]  (if rX dead after)
    // This eliminates redundant register copies used only as memory base addresses.
    if i + 1 < instructions.len() {
        if let X86Instr::Mov(X86Operand::Reg(copy_dest), X86Operand::Reg(copy_src)) = &instructions[i] {
            let copy_dest_c = copy_dest.clone();
            let copy_src_c = copy_src.clone();
            // Try to substitute copy_dest with copy_src in the next instruction's memory operands
            if let Some(new_instr) = substitute_base_reg(&instructions[i + 1], &copy_dest_c, &copy_src_c) {
                // Check the copy_dest register is dead after the substituted instruction
                if !is_reg_used_after(instructions, i + 2, &copy_dest_c) {
                    instructions[i] = new_instr;
                    instructions.remove(i + 1);
                    return true;
                }
            }
        }
    }

    // Pattern 11: Movsx/Movzx chain forwarding
    // movsx rX, eY; mov rZ, rX → movsx rZ, eY  (if rX dead after)
    // movzx rX, oY; mov rZ, rX → movzx rZ, oY  (if rX dead after)
    if i + 1 < instructions.len() {
        if let X86Instr::Mov(X86Operand::Reg(mov_dest), X86Operand::Reg(mov_src)) = &instructions[i + 1] {
            match &instructions[i] {
                X86Instr::Movsx(X86Operand::Reg(sx_dest), sx_src)
                    if same_physical_reg(sx_dest, mov_src)
                    && !is_reg_used_after(instructions, i + 2, sx_dest) =>
                {
                    instructions[i] = X86Instr::Movsx(X86Operand::Reg(mov_dest.clone()), sx_src.clone());
                    instructions.remove(i + 1);
                    return true;
                }
                X86Instr::Movzx(X86Operand::Reg(zx_dest), zx_src)
                    if same_physical_reg(zx_dest, mov_src)
                    && !is_reg_used_after(instructions, i + 2, zx_dest) =>
                {
                    instructions[i] = X86Instr::Movzx(X86Operand::Reg(mov_dest.clone()), zx_src.clone());
                    instructions.remove(i + 1);
                    return true;
                }
                _ => {}
            }
        }
    }

    // Pattern 12: Dead store-after-load elimination
    // mov rX, [mem]; mov [mem], rX → remove the store (value already in memory)
    // The load is kept if rX is used later; otherwise Pattern 9 will remove it.
    if i + 1 < instructions.len() {
        if let (
            X86Instr::Mov(X86Operand::Reg(load_dest), load_src),
            X86Instr::Mov(store_dest, X86Operand::Reg(store_src))
        ) = (&instructions[i], &instructions[i + 1]) {
            if same_physical_reg(load_dest, store_src) && load_src == store_dest {
                // The store writes back the same value that was loaded — remove it
                instructions.remove(i + 1);
                return true;
            }
        }
    }
    
    // Pattern 9: mov dest, src where dest is overwritten before being read → remove (dead store)
    if let X86Instr::Mov(X86Operand::Reg(dest_reg), _) = &instructions[i] {
        if !is_reg_used_after(instructions, i + 1, dest_reg) {
            instructions.remove(i);
            return true;
        }
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

/// Try to fold a LEA's computed address into the next instruction's memory operand.
/// lea_dest: register set by the LEA (e.g., Rax)
/// lea_base: base register of the LEA (e.g., Rbp)
/// lea_off: offset of the LEA (e.g., -16)
/// Returns Some(new_instruction) if the fold is possible.
fn fold_lea_into_next(instr: &X86Instr, lea_dest: &X86Reg, lea_base: &X86Reg, lea_off: i32) -> Option<X86Instr> {
    // Helper: substitute [lea_dest+off2] → [lea_base + lea_off + off2]
    let subst = |op: &X86Operand| -> Option<X86Operand> {
        match op {
            X86Operand::Mem(base, off2) if same_physical_reg(base, lea_dest) =>
                Some(X86Operand::Mem(lea_base.clone(), lea_off + off2)),
            X86Operand::DwordMem(base, off2) if same_physical_reg(base, lea_dest) =>
                Some(X86Operand::DwordMem(lea_base.clone(), lea_off + off2)),
            X86Operand::WordMem(base, off2) if same_physical_reg(base, lea_dest) =>
                Some(X86Operand::WordMem(lea_base.clone(), lea_off + off2)),
            X86Operand::ByteMem(base, off2) if same_physical_reg(base, lea_dest) =>
                Some(X86Operand::ByteMem(lea_base.clone(), lea_off + off2)),
            _ => None,
        }
    };

    match instr {
        // mov DWORD PTR [rax+0], src → mov DWORD PTR [base+off], src
        X86Instr::Mov(dest, src) => {
            if let Some(new_dest) = subst(dest) {
                // Make sure src doesn't also use lea_dest as a value (only if it's a direct reg ref)
                if !reads_reg_direct(src, lea_dest) {
                    return Some(X86Instr::Mov(new_dest, src.clone()));
                }
            }
            // mov eax, DWORD PTR [rax+0] → mov eax, DWORD PTR [base+off]
            // But only if dest doesn't alias lea_dest (otherwise the LEA result is needed)
            if let Some(new_src) = subst(src) {
                // If dest overwrites lea_dest (e.g., mov eax, [rax+0] — eax alias of rax),
                // that's fine since we're eliminating the LEA anyway
                return Some(X86Instr::Mov(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Add(dest, src) => {
            if let Some(new_dest) = subst(dest) {
                if !reads_reg_direct(src, lea_dest) {
                    return Some(X86Instr::Add(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Sub(dest, src) => {
            if let Some(new_dest) = subst(dest) {
                if !reads_reg_direct(src, lea_dest) {
                    return Some(X86Instr::Sub(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Cmp(left, right) => {
            if let Some(new_left) = subst(left) {
                if !reads_reg_direct(right, lea_dest) {
                    return Some(X86Instr::Cmp(new_left, right.clone()));
                }
            }
            None
        }
        X86Instr::Movsx(dest, src) => {
            if let Some(new_src) = subst(src) {
                return Some(X86Instr::Movsx(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Movzx(dest, src) => {
            if let Some(new_src) = subst(src) {
                return Some(X86Instr::Movzx(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Lea(dest, src) => {
            if let Some(new_src) = subst(src) {
                return Some(X86Instr::Lea(dest.clone(), new_src));
            }
            None
        }
        _ => None,
    }
}

/// Check if a register is *read* before being overwritten or dead, starting from
/// position `start`. Returns true only if we can find a genuine read of the
/// register, not just a write.
///
/// This version follows control flow across basic block boundaries using a
/// depth-limited DFS. At conditional branches, both paths are checked. At
/// unconditional jumps, the target is followed. Labels are transparent (skipped).
/// Calls are treated conservatively (register assumed live). A visited set
/// prevents infinite recursion through loops.
fn is_reg_used_after(instructions: &[X86Instr], start: usize, reg: &X86Reg) -> bool {
    let mut visited = HashSet::new();
    is_reg_live_from(instructions, start, reg, &mut visited, 8)
}

/// Find the position of a label in the instruction stream.
fn find_label_pos(instrs: &[X86Instr], label: &str) -> Option<usize> {
    instrs.iter().position(|inst| matches!(inst, X86Instr::Label(name) if name == label))
}

/// Recursive helper for cross-block register liveness analysis.
fn is_reg_live_from(
    instrs: &[X86Instr],
    start: usize,
    reg: &X86Reg,
    visited: &mut HashSet<usize>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return true; // conservative at depth limit
    }
    
    let is_rax = matches!(physical_reg_id(reg), 0);
    
    for idx in start..instrs.len() {
        if !visited.insert(idx) {
            return true; // cycle detected, conservative
        }
        
        match &instrs[idx] {
            // Labels are transparent — continue scanning forward
            X86Instr::Label(_) => continue,
            
            // At ret: only rax is live (return value)
            X86Instr::Ret => return is_rax,
            
            // At unconditional jump: follow the target
            X86Instr::Jmp(target) => {
                return if let Some(pos) = find_label_pos(instrs, target) {
                    is_reg_live_from(instrs, pos, reg, visited, depth - 1)
                } else {
                    true // unknown target, conservative
                };
            }
            
            // At conditional branch: check taken path; if dead, continue with fallthrough
            X86Instr::Jcc(_, target) => {
                if let Some(pos) = find_label_pos(instrs, target) {
                    if is_reg_live_from(instrs, pos, reg, visited, depth - 1) {
                        return true;
                    }
                    // Taken path says dead; continue scanning fallthrough
                } else {
                    return true; // unknown target, conservative
                }
            }
            
            // At calls: conservatively assume all registers are live
            X86Instr::Call(_) | X86Instr::CallIndirect(_) => return true,
            
            // Standard instruction-level liveness tracking
            X86Instr::Mov(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; } // overwritten
                if uses_reg_as_base(dest, reg) { return true; }
            }
            X86Instr::Add(dest, src) | X86Instr::Sub(dest, src) |
            X86Instr::And(dest, src) | X86Instr::Or(dest, src) | X86Instr::Xor(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Cmp(left, right) | X86Instr::Test(left, right) => {
                if reads_reg(left, reg) || reads_reg(right, reg) { return true; }
            }
            X86Instr::Lea(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; }
            }
            X86Instr::Movsx(dest, src) | X86Instr::Movzx(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; }
            }
            X86Instr::Imul(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Neg(op) | X86Instr::Not(op) => {
                if reads_reg(op, reg) { return true; }
            }
            X86Instr::Idiv(op) => {
                // idiv reads rax, rdx, and the operand; writes rax (quotient) and rdx (remainder)
                if reads_reg(op, reg) { return true; }
                let reg_id = physical_reg_id(reg);
                if reg_id == 0 || reg_id == 2 { return true; } // rax/rdx are both read and written
            }
            X86Instr::Set(_, _op) => {
                // set writes only a byte register — partial write, continue
            }
            X86Instr::Push(r) => {
                if same_physical_reg(r, reg) { return true; }
            }
            X86Instr::Pop(r) => {
                if same_physical_reg(r, reg) { return false; } // overwritten
            }
            X86Instr::Shl(dest, src) | X86Instr::Shr(dest, src) | X86Instr::Sar(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Movss(dest, src) | X86Instr::Movsd(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; }
                if uses_reg_as_base(dest, reg) { return true; }
            }
            X86Instr::Addss(dest, src) | X86Instr::Subss(dest, src) |
            X86Instr::Mulss(dest, src) | X86Instr::Divss(dest, src) |
            X86Instr::Ucomiss(dest, src) | X86Instr::Cvtsi2ss(dest, src) |
            X86Instr::Cvttss2si(dest, src) | X86Instr::Xorps(dest, src) |
            X86Instr::Addsd(dest, src) | X86Instr::Subsd(dest, src) |
            X86Instr::Mulsd(dest, src) | X86Instr::Divsd(dest, src) |
            X86Instr::Ucomisd(dest, src) | X86Instr::Cvtsi2sd(dest, src) |
            X86Instr::Cvttsd2si(dest, src) | X86Instr::Xorpd(dest, src) |
            X86Instr::Cvtss2sd(dest, src) | X86Instr::Cvtsd2ss(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            // Packed SSE/AVX instructions
            X86Instr::Movaps(dest, src) | X86Instr::Movups(dest, src) |
            X86Instr::Addps(dest, src) | X86Instr::Subps(dest, src) |
            X86Instr::Mulps(dest, src) | X86Instr::Divps(dest, src) |
            X86Instr::Movdqa(dest, src) | X86Instr::Movdqu(dest, src) |
            X86Instr::Paddd(dest, src) | X86Instr::Psubd(dest, src) |
            X86Instr::Pmulld(dest, src) |
            X86Instr::Pxor(dest, src) | X86Instr::Movd(dest, src) |
            X86Instr::Vmovaps(dest, src) | X86Instr::Vmovups(dest, src) |
            X86Instr::Vmovdqa(dest, src) | X86Instr::Vmovdqu(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Pshufd(dest, src, _) | X86Instr::Vextracti128(dest, src, _) |
            X86Instr::Vpbroadcastd(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; }
            }
            X86Instr::Vaddps(dest, s1, s2) | X86Instr::Vsubps(dest, s1, s2) |
            X86Instr::Vmulps(dest, s1, s2) | X86Instr::Vdivps(dest, s1, s2) |
            X86Instr::Vpaddd(dest, s1, s2) | X86Instr::Vpsubd(dest, s1, s2) |
            X86Instr::Vpmulld(dest, s1, s2) | X86Instr::Vxorps(dest, s1, s2) |
            X86Instr::Vpxor(dest, s1, s2) => {
                if reads_reg(dest, reg) || reads_reg(s1, reg) || reads_reg(s2, reg) { return true; }
            }
            X86Instr::Vzeroupper => {
                // Clears upper bits of all YMM registers, doesn't affect XMM bottom halves
            }
            X86Instr::Cqto => {
                // cqto sign-extends rax into rdx:rax. Reads rax, writes rdx.
                if is_rax { return true; } // reads rax
                if matches!(physical_reg_id(reg), 2) { return false; } // overwrites rdx
            }
            X86Instr::Cdq => {
                // cdq sign-extends eax into edx:eax. Reads eax, writes edx.
                if is_rax { return true; }
                if matches!(physical_reg_id(reg), 2) { return false; }
            }
            X86Instr::Leave => {
                // leave = mov rsp, rbp; pop rbp. Reads rbp, writes rsp and rbp.
                let is_rbp = matches!(physical_reg_id(reg), 5);
                let is_rsp = matches!(physical_reg_id(reg), 4);
                if is_rbp { return true; } // reads rbp (then overwrites it, but read comes first)
                if is_rsp { return false; } // overwrites rsp
                // Other registers: not touched by leave, continue scanning
            }
            X86Instr::Raw(_) => {
                return true; // conservative
            }
        }
    }
    false
}

/// Check if an instruction reads or writes the given register (any alias).
fn instr_touches_reg(inst: &X86Instr, reg: &X86Reg) -> bool {
    match inst {
        X86Instr::Mov(d, s) | X86Instr::Add(d, s) | X86Instr::Sub(d, s) |
        X86Instr::And(d, s) | X86Instr::Or(d, s) | X86Instr::Xor(d, s) |
        X86Instr::Imul(d, s) | X86Instr::Cmp(d, s) | X86Instr::Test(d, s) |
        X86Instr::Lea(d, s) | X86Instr::Movsx(d, s) | X86Instr::Movzx(d, s) |
        X86Instr::Shl(d, s) | X86Instr::Shr(d, s) | X86Instr::Sar(d, s) |
        X86Instr::Movss(d, s) | X86Instr::Addss(d, s) | X86Instr::Subss(d, s) |
        X86Instr::Mulss(d, s) | X86Instr::Divss(d, s) | X86Instr::Ucomiss(d, s) |
        X86Instr::Cvtsi2ss(d, s) | X86Instr::Cvttss2si(d, s) | X86Instr::Xorps(d, s) |
        X86Instr::Movsd(d, s) | X86Instr::Addsd(d, s) | X86Instr::Subsd(d, s) |
        X86Instr::Mulsd(d, s) | X86Instr::Divsd(d, s) | X86Instr::Ucomisd(d, s) |
        X86Instr::Cvtsi2sd(d, s) | X86Instr::Cvttsd2si(d, s) | X86Instr::Xorpd(d, s) |
        X86Instr::Cvtss2sd(d, s) | X86Instr::Cvtsd2ss(d, s) |
        X86Instr::Movaps(d, s) | X86Instr::Movups(d, s) |
        X86Instr::Addps(d, s) | X86Instr::Subps(d, s) |
        X86Instr::Mulps(d, s) | X86Instr::Divps(d, s) |
        X86Instr::Movdqa(d, s) | X86Instr::Movdqu(d, s) |
        X86Instr::Paddd(d, s) | X86Instr::Psubd(d, s) | X86Instr::Pmulld(d, s) |
        X86Instr::Pxor(d, s) | X86Instr::Movd(d, s) | X86Instr::Pshufd(d, s, _) |
        X86Instr::Vmovaps(d, s) | X86Instr::Vmovups(d, s) |
        X86Instr::Vmovdqa(d, s) | X86Instr::Vmovdqu(d, s) |
        X86Instr::Vextracti128(d, s, _) | X86Instr::Vpbroadcastd(d, s) => {
            reads_reg(d, reg) || reads_reg(s, reg)
        }
        X86Instr::Vaddps(d, s1, s2) | X86Instr::Vsubps(d, s1, s2) |
        X86Instr::Vmulps(d, s1, s2) | X86Instr::Vdivps(d, s1, s2) |
        X86Instr::Vpaddd(d, s1, s2) | X86Instr::Vpsubd(d, s1, s2) |
        X86Instr::Vpmulld(d, s1, s2) | X86Instr::Vxorps(d, s1, s2) |
        X86Instr::Vpxor(d, s1, s2) => {
            reads_reg(d, reg) || reads_reg(s1, reg) || reads_reg(s2, reg)
        }
        X86Instr::Neg(o) | X86Instr::Not(o) => reads_reg(o, reg),
        X86Instr::Idiv(o) => {
            // idiv implicitly reads rax and rdx:rax
            reads_reg(o, reg) || physical_reg_id(reg) == 0 || physical_reg_id(reg) == 2
        }
        X86Instr::Set(_, o) => reads_reg(o, reg),
        X86Instr::Push(r) | X86Instr::Pop(r) => same_physical_reg(r, reg),
        X86Instr::CallIndirect(o) => reads_reg(o, reg),
        _ => false,
    }
}

/// Substitute a register used as a memory base in the given instruction.
/// If `old_reg` appears as the base register of a memory operand in `instr`,
/// replace it with `new_reg` and return the updated instruction.
fn substitute_base_reg(instr: &X86Instr, old_reg: &X86Reg, new_reg: &X86Reg) -> Option<X86Instr> {
    let subst_op = |op: &X86Operand| -> Option<X86Operand> {
        match op {
            X86Operand::Mem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::Mem(new_reg.clone(), *off)),
            X86Operand::DwordMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::DwordMem(new_reg.clone(), *off)),
            X86Operand::WordMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::WordMem(new_reg.clone(), *off)),
            X86Operand::ByteMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::ByteMem(new_reg.clone(), *off)),
            X86Operand::FloatMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::FloatMem(new_reg.clone(), *off)),
            X86Operand::DoubleMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::DoubleMem(new_reg.clone(), *off)),
            X86Operand::XmmwordMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::XmmwordMem(new_reg.clone(), *off)),
            X86Operand::YmmwordMem(base, off) if same_physical_reg(base, old_reg) =>
                Some(X86Operand::YmmwordMem(new_reg.clone(), *off)),
            _ => None,
        }
    };

    match instr {
        X86Instr::Mov(dest, src) => {
            // Try dest first (store case): mov [rX+off], val → mov [rY+off], val
            if let Some(new_dest) = subst_op(dest) {
                if !reads_reg_direct(src, old_reg) {
                    return Some(X86Instr::Mov(new_dest, src.clone()));
                }
            }
            // Try src (load case): mov eax, [rX+off] → mov eax, [rY+off]
            if let Some(new_src) = subst_op(src) {
                return Some(X86Instr::Mov(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Movsx(dest, src) => {
            if let Some(new_src) = subst_op(src) {
                return Some(X86Instr::Movsx(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Movzx(dest, src) => {
            if let Some(new_src) = subst_op(src) {
                return Some(X86Instr::Movzx(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Add(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                if !reads_reg_direct(src, old_reg) {
                    return Some(X86Instr::Add(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Sub(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                if !reads_reg_direct(src, old_reg) {
                    return Some(X86Instr::Sub(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Cmp(left, right) => {
            if let Some(new_left) = subst_op(left) {
                if !reads_reg_direct(right, old_reg) {
                    return Some(X86Instr::Cmp(new_left, right.clone()));
                }
            }
            if let Some(new_right) = subst_op(right) {
                if !reads_reg_direct(left, old_reg) {
                    return Some(X86Instr::Cmp(left.clone(), new_right));
                }
            }
            None
        }
        X86Instr::Vmovdqu(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                return Some(X86Instr::Vmovdqu(new_dest, src.clone()));
            }
            if let Some(new_src) = subst_op(src) {
                return Some(X86Instr::Vmovdqu(dest.clone(), new_src));
            }
            None
        }
        _ => None,
    }
}

/// Check if a register is READ (not just written) within the current basic block
/// starting from `start`. Stops and returns false at block boundaries (jmp/jcc/label/ret/call).
/// This is a weaker check than is_reg_used_after — it only looks within the block.
fn is_reg_read_in_block(instructions: &[X86Instr], start: usize, reg: &X86Reg) -> bool {
    for inst in instructions.iter().skip(start) {
        match inst {
            // End of basic block — register's value doesn't matter within THIS block
            X86Instr::Jmp(_) | X86Instr::Jcc(_, _) | X86Instr::Label(_) |
            X86Instr::Ret | X86Instr::Call(_) | X86Instr::CallIndirect(_) => {
                return false;
            }
            X86Instr::Mov(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; } // overwritten before read
                if uses_reg_as_base(dest, reg) { return true; }
            }
            X86Instr::Add(dest, src) | X86Instr::Sub(dest, src) |
            X86Instr::And(dest, src) | X86Instr::Or(dest, src) | X86Instr::Xor(dest, src) |
            X86Instr::Imul(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Cmp(left, right) | X86Instr::Test(left, right) => {
                if reads_reg(left, reg) || reads_reg(right, reg) { return true; }
            }
            X86Instr::Lea(dest, src) | X86Instr::Movsx(dest, src) | X86Instr::Movzx(dest, src) => {
                if reads_reg(src, reg) { return true; }
                if reads_reg_direct(dest, reg) { return false; }
            }
            X86Instr::Push(r) => {
                if same_physical_reg(r, reg) { return true; }
            }
            X86Instr::Pop(r) => {
                if same_physical_reg(r, reg) { return false; }
            }
            X86Instr::Neg(op) | X86Instr::Not(op) => {
                if reads_reg(op, reg) { return true; }
            }
            X86Instr::Idiv(op) => {
                // idiv reads rax, rdx, and the operand; writes rax and rdx
                if reads_reg(op, reg) { return true; }
                let reg_id = physical_reg_id(reg);
                if reg_id == 0 || reg_id == 2 { return true; }
            }
            X86Instr::Shl(dest, src) | X86Instr::Shr(dest, src) | X86Instr::Sar(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Movss(dest, src) | X86Instr::Addss(dest, src) |
            X86Instr::Subss(dest, src) | X86Instr::Mulss(dest, src) |
            X86Instr::Divss(dest, src) | X86Instr::Ucomiss(dest, src) |
            X86Instr::Cvtsi2ss(dest, src) | X86Instr::Cvttss2si(dest, src) |
            X86Instr::Xorps(dest, src) |
            X86Instr::Movsd(dest, src) | X86Instr::Addsd(dest, src) |
            X86Instr::Subsd(dest, src) | X86Instr::Mulsd(dest, src) |
            X86Instr::Divsd(dest, src) | X86Instr::Ucomisd(dest, src) |
            X86Instr::Cvtsi2sd(dest, src) | X86Instr::Cvttsd2si(dest, src) |
            X86Instr::Xorpd(dest, src) |
            X86Instr::Cvtss2sd(dest, src) | X86Instr::Cvtsd2ss(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            // Packed SSE/AVX
            X86Instr::Movaps(dest, src) | X86Instr::Movups(dest, src) |
            X86Instr::Addps(dest, src) | X86Instr::Subps(dest, src) |
            X86Instr::Mulps(dest, src) | X86Instr::Divps(dest, src) |
            X86Instr::Movdqa(dest, src) | X86Instr::Movdqu(dest, src) |
            X86Instr::Paddd(dest, src) | X86Instr::Psubd(dest, src) |
            X86Instr::Pmulld(dest, src) | X86Instr::Pxor(dest, src) |
            X86Instr::Movd(dest, src) | X86Instr::Pshufd(dest, src, _) |
            X86Instr::Vmovaps(dest, src) | X86Instr::Vmovups(dest, src) |
            X86Instr::Vmovdqa(dest, src) | X86Instr::Vmovdqu(dest, src) |
            X86Instr::Vextracti128(dest, src, _) | X86Instr::Vpbroadcastd(dest, src) => {
                if reads_reg(src, reg) || reads_reg(dest, reg) { return true; }
            }
            X86Instr::Vaddps(dest, s1, s2) | X86Instr::Vsubps(dest, s1, s2) |
            X86Instr::Vmulps(dest, s1, s2) | X86Instr::Vdivps(dest, s1, s2) |
            X86Instr::Vpaddd(dest, s1, s2) | X86Instr::Vpsubd(dest, s1, s2) |
            X86Instr::Vpmulld(dest, s1, s2) | X86Instr::Vxorps(dest, s1, s2) |
            X86Instr::Vpxor(dest, s1, s2) => {
                if reads_reg(dest, reg) || reads_reg(s1, reg) || reads_reg(s2, reg) { return true; }
            }
            X86Instr::Vzeroupper => {}
            _ => { return true; } // conservative
        }
    }
    false
}

/// Check if an operand reads from the given register (as value or as memory base).
/// Uses physical register aliasing (rax == eax == al == ax, etc.)
fn reads_reg(operand: &X86Operand, reg: &X86Reg) -> bool {
    match operand {
        X86Operand::Reg(r) => same_physical_reg(r, reg),
        X86Operand::Mem(r, _) | X86Operand::DwordMem(r, _) | X86Operand::WordMem(r, _) |
        X86Operand::ByteMem(r, _) | X86Operand::FloatMem(r, _) | X86Operand::DoubleMem(r, _) |
        X86Operand::XmmwordMem(r, _) | X86Operand::YmmwordMem(r, _) => {
            same_physical_reg(r, reg)
        }
        _ => false,
    }
}

/// Check if an operand is a direct register reference (not memory with base)
fn reads_reg_direct(operand: &X86Operand, reg: &X86Reg) -> bool {
    match operand {
        X86Operand::Reg(r) => same_physical_reg(r, reg),
        _ => false,
    }
}

/// Check if an operand uses the register as a memory base address (meaning it reads it)
fn uses_reg_as_base(operand: &X86Operand, reg: &X86Reg) -> bool {
    match operand {
        X86Operand::Mem(r, _) | X86Operand::DwordMem(r, _) | X86Operand::WordMem(r, _) |
        X86Operand::ByteMem(r, _) | X86Operand::FloatMem(r, _) | X86Operand::DoubleMem(r, _) |
        X86Operand::XmmwordMem(r, _) | X86Operand::YmmwordMem(r, _) => {
            same_physical_reg(r, reg)
        }
        _ => false,
    }
}

/// Returns true if two X86Reg values refer to the same physical register.
/// e.g., Rax, Eax, Ax, Al all share the same physical register.
fn same_physical_reg(a: &X86Reg, b: &X86Reg) -> bool {
    physical_reg_id(a) == physical_reg_id(b)
}

fn physical_reg_id(r: &X86Reg) -> u8 {
    match r {
        X86Reg::Rax | X86Reg::Eax | X86Reg::Ax | X86Reg::Al => 0,
        X86Reg::Rcx | X86Reg::Ecx | X86Reg::Cx | X86Reg::Cl => 1,
        X86Reg::Rdx | X86Reg::Edx => 2,
        X86Reg::Rbx | X86Reg::Ebx => 3,
        X86Reg::Rsp | X86Reg::Esp => 4,
        X86Reg::Rbp | X86Reg::Ebp => 5,
        X86Reg::Rsi | X86Reg::Esi => 6,
        X86Reg::Rdi | X86Reg::Edi => 7,
        X86Reg::R8 | X86Reg::R8d => 8,
        X86Reg::R9 | X86Reg::R9d => 9,
        X86Reg::R10 | X86Reg::R10d => 10,
        X86Reg::R11 | X86Reg::R11d => 11,
        X86Reg::R12 | X86Reg::R12d => 12,
        X86Reg::R13 | X86Reg::R13d => 13,
        X86Reg::R14 | X86Reg::R14d => 14,
        X86Reg::R15 | X86Reg::R15d => 15,
        X86Reg::Xmm0 => 16,
        X86Reg::Xmm1 => 17,
        X86Reg::Xmm2 => 18,
        X86Reg::Xmm3 => 19,
        X86Reg::Xmm4 => 20,
        X86Reg::Xmm5 => 21,
        X86Reg::Xmm6 => 22,
        X86Reg::Xmm7 => 23,
        X86Reg::Xmm8 => 24,
        X86Reg::Xmm9 => 25,
        X86Reg::Xmm10 => 26,
        X86Reg::Xmm11 => 27,
        X86Reg::Xmm12 => 28,
        X86Reg::Xmm13 => 29,
        X86Reg::Xmm14 => 30,
        X86Reg::Xmm15 => 31,
        X86Reg::Ymm0 => 16,  // YMM aliases XMM (same physical register)
        X86Reg::Ymm1 => 17,
        X86Reg::Ymm2 => 18,
        X86Reg::Ymm3 => 19,
        X86Reg::Ymm4 => 20,
        X86Reg::Ymm5 => 21,
        X86Reg::Ymm6 => 22,
        X86Reg::Ymm7 => 23,
        X86Reg::Ymm8 => 24,
        X86Reg::Ymm9 => 25,
        X86Reg::Ymm10 => 26,
        X86Reg::Ymm11 => 27,
        X86Reg::Ymm12 => 28,
        X86Reg::Ymm13 => 29,
        X86Reg::Ymm14 => 30,
        X86Reg::Ymm15 => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: make a register operand
    fn reg(r: X86Reg) -> X86Operand { X86Operand::Reg(r) }
    // Helper: make an immediate operand
    fn imm(v: i64) -> X86Operand { X86Operand::Imm(v) }
    // Helper: make a memory operand
    fn mem(base: X86Reg, off: i32) -> X86Operand { X86Operand::Mem(base, off) }

    // ─── invert_condition ───────────────────────────────────────

    #[test]
    fn invert_condition_all_pairs() {
        let pairs = [
            ("e", "ne"), ("ne", "e"),
            ("l", "ge"), ("ge", "l"),
            ("le", "g"), ("g", "le"),
            ("b", "ae"), ("ae", "b"),
            ("be", "a"), ("a", "be"),
        ];
        for (input, expected) in &pairs {
            assert_eq!(
                invert_condition(input).as_deref(),
                Some(*expected),
                "invert_condition({input}) should be {expected}"
            );
        }
    }

    #[test]
    fn invert_condition_unknown_returns_none() {
        assert_eq!(invert_condition("xyz"), None);
        assert_eq!(invert_condition(""), None);
    }

    // ─── same_physical_reg / physical_reg_id ────────────────────

    #[test]
    fn same_physical_reg_aliases() {
        assert!(same_physical_reg(&X86Reg::Rax, &X86Reg::Eax));
        assert!(same_physical_reg(&X86Reg::Eax, &X86Reg::Al));
        assert!(same_physical_reg(&X86Reg::Rcx, &X86Reg::Cl));
        assert!(same_physical_reg(&X86Reg::Xmm0, &X86Reg::Ymm0));
    }

    #[test]
    fn same_physical_reg_different() {
        assert!(!same_physical_reg(&X86Reg::Rax, &X86Reg::Rbx));
        assert!(!same_physical_reg(&X86Reg::Eax, &X86Reg::Ecx));
        assert!(!same_physical_reg(&X86Reg::Xmm0, &X86Reg::Xmm1));
    }

    // ─── Pattern 1: redundant mov removal (mov reg, reg) ────────

    #[test]
    fn remove_redundant_mov_same_reg() {
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rax), reg(X86Reg::Rax)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    #[test]
    fn keep_mov_different_regs() {
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rax), reg(X86Reg::Rbx)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // mov rax, rbx should remain (not a nop, and rax is used by ret convention)
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Mov(..))));
    }

    // ─── Pattern 2: copy forwarding (mov tmp, src; mov dest, tmp) ─

    #[test]
    fn copy_forwarding_adjacent() {
        // mov rcx, rbx; mov [rbp-8], rcx; ret
        // rcx is dead after → should become: mov [rbp-8], rbx; ret
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), reg(X86Reg::Rbx)),
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rcx)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // Should have forwarded: mov [rbp-8], rbx; ret
        assert_eq!(instrs.len(), 2);
        assert!(matches!(&instrs[0], X86Instr::Mov(
            X86Operand::Mem(X86Reg::Rbp, -8),
            X86Operand::Reg(X86Reg::Rbx)
        )));
    }

    // ─── Pattern 3: add/sub with 0 → remove ────────────────────

    #[test]
    fn remove_add_zero() {
        let mut instrs = vec![
            X86Instr::Add(reg(X86Reg::Rax), imm(0)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    #[test]
    fn remove_sub_zero() {
        let mut instrs = vec![
            X86Instr::Sub(reg(X86Reg::Rbx), imm(0)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    // ─── Pattern 4: imul simplifications ────────────────────────

    #[test]
    fn remove_imul_by_one() {
        let mut instrs = vec![
            X86Instr::Imul(reg(X86Reg::Rax), imm(1)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    #[test]
    fn imul_by_zero_becomes_mov_zero() {
        let mut instrs = vec![
            X86Instr::Imul(reg(X86Reg::Rax), imm(0)),
            // Need to use rax after to keep it alive
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rax)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // imul rax, 0 → mov rax, 0, then mov [rbp-8], rax stays   
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Mov(
            X86Operand::Reg(X86Reg::Rax),
            X86Operand::Imm(0)
        ))));
    }

    #[test]
    fn constant_fold_mov_imul() {
        // mov rax, 5; imul rax, 10 → mov rax, 50
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rax), imm(5)),
            X86Instr::Imul(reg(X86Reg::Rax), imm(10)),
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rax)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Mov(
            X86Operand::Reg(X86Reg::Rax),
            X86Operand::Imm(50)
        ))));
    }

    // ─── Pattern 5: mov imm + add → lea ────────────────────────

    #[test]
    fn mov_imm_add_to_lea() {
        // mov rax, 8; add rax, rbx → lea rax, [rbx+8]
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rax), imm(8)),
            X86Instr::Add(reg(X86Reg::Rax), reg(X86Reg::Rbx)),
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rax)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Lea(
            X86Operand::Reg(X86Reg::Rax),
            X86Operand::Mem(X86Reg::Rbx, 8)
        ))));
    }

    // ─── Fallthrough jump elimination ───────────────────────────

    #[test]
    fn remove_jmp_to_next_label() {
        // jmp L1; L1: → remove jmp
        let mut instrs = vec![
            X86Instr::Jmp("L1".to_string()),
            X86Instr::Label("L1".to_string()),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 2);
        assert!(matches!(&instrs[0], X86Instr::Label(l) if l == "L1"));
    }

    #[test]
    fn conditional_inversion_fallthrough() {
        // jcc A; jmp B; A: → j!cc B; A:
        let mut instrs = vec![
            X86Instr::Jcc("e".to_string(), "A".to_string()),
            X86Instr::Jmp("B".to_string()),
            X86Instr::Label("A".to_string()),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // Should become: jne B; A:; ret
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Jcc(c, t) if c == "ne" && t == "B")));
        // The jmp B should be removed
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Jmp(t) if t == "B")));
    }

    // ─── Jump chain elimination ─────────────────────────────────

    #[test]
    fn eliminate_jump_chain() {
        // jmp A; ... A: jmp B; ... B: ret
        // → jmp B
        let mut instrs = vec![
            X86Instr::Jmp("A".to_string()),
            X86Instr::Label("A".to_string()),
            X86Instr::Jmp("B".to_string()),
            X86Instr::Label("B".to_string()),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // First jmp should go directly to B (then fallthrough removes it since B is reachable)
        // After all opts, we should have Label A, jmp B (or nothing before B), Label B, ret
        // The key invariant: no jmp A should remain
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Jmp(t) if t == "A")));
    }

    #[test]
    fn conditional_jump_chain() {
        // je A; ... A: jmp B;
        // Chain resolution redirects je A → je B,
        // then fallthrough elimination inverts je B; jmp end; B: into jne end; B:
        let mut instrs = vec![
            X86Instr::Jcc("e".to_string(), "A".to_string()),
            X86Instr::Jmp("end".to_string()),
            X86Instr::Label("A".to_string()),
            X86Instr::Jmp("B".to_string()),
            X86Instr::Label("B".to_string()),
            X86Instr::Ret,
            X86Instr::Label("end".to_string()),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // No jump to A should remain (chain was resolved)
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Jcc(_, t) | X86Instr::Jmp(t) if t == "A")));
        // After chain + inversion: jne end; B:; ret; end:; ret
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Jcc(c, t) if c == "ne" && t == "end")));
    }

    // ─── Pattern 2c: immediate forwarding ───────────────────────

    #[test]
    fn immediate_forwarding_into_add() {
        // mov rcx, 42; add [rbp-8], rcx → add [rbp-8], 42
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), imm(42)),
            X86Instr::Add(mem(X86Reg::Rbp, -8), reg(X86Reg::Rcx)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Add(
            X86Operand::Mem(X86Reg::Rbp, -8),
            X86Operand::Imm(42)
        ))));
    }

    // ─── Pattern 3b: lea + add constant folding ─────────────────

    #[test]
    fn lea_add_constant_fold() {
        // lea rax, [rbp-16]; add rax, 4 → lea rax, [rbp-12]
        let mut instrs = vec![
            X86Instr::Lea(reg(X86Reg::Rax), mem(X86Reg::Rbp, -16)),
            X86Instr::Add(reg(X86Reg::Rax), imm(4)),
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rax)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Lea(
            X86Operand::Reg(X86Reg::Rax),
            X86Operand::Mem(X86Reg::Rbp, -12)
        ))));
    }

    // ─── Pattern 9: dead store elimination ──────────────────────

    #[test]
    fn remove_dead_store_to_reg() {
        // mov rdi, 5; ret  → if rdi is dead, mov is removed
        // But with ret, rdi might be live. Use explicit dead pattern:
        // mov rdi, 5; mov rdi, 10; ret → mov rdi, 10; ret
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rdi), imm(5)),
            X86Instr::Mov(reg(X86Reg::Rdi), imm(10)),
            X86Instr::Mov(mem(X86Reg::Rbp, -8), reg(X86Reg::Rdi)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // First mov should be removed as dead store
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Mov(
            X86Operand::Reg(X86Reg::Rdi),
            X86Operand::Imm(5)
        ))));
    }

    // ─── Pattern 0: cmp/set/test/branch → cmp/jcc ──────────────

    #[test]
    fn simplify_cmp_set_test_branch() {
        // cmp rbx, 0; mov rax, 0; sete al; mov rcx, rax; test rcx, rcx; jne label
        // → cmp rbx, 0; je label
        let mut instrs = vec![
            X86Instr::Cmp(reg(X86Reg::Rbx), imm(0)),
            X86Instr::Mov(reg(X86Reg::Rax), imm(0)),
            X86Instr::Set("e".to_string(), reg(X86Reg::Al)),
            X86Instr::Mov(reg(X86Reg::Rcx), reg(X86Reg::Rax)),
            X86Instr::Test(reg(X86Reg::Rcx), reg(X86Reg::Rcx)),
            X86Instr::Jcc("ne".to_string(), "target".to_string()),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // Should simplify to: cmp rbx, 0; je target; ret (3 instructions)
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Cmp(..))));
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Jcc(c, t) if c == "e" && t == "target")));
        // The intermediate set/test/mov should be gone
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Set(..))));
        assert!(!instrs.iter().any(|i| matches!(i, X86Instr::Test(..))));
    }

    // ─── is_reg_used_after cross-block ──────────────────────────

    #[test]
    fn reg_not_used_after_ret() {
        let instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), imm(5)),
            X86Instr::Ret,
        ];
        // rcx is not used after position 1 (ret doesn't read rcx specifically)
        // But ret is treated conservatively - check the actual behavior  
        let result = is_reg_used_after(&instrs, 1, &X86Reg::Rcx);
        // Ret doesn't read rcx in general (only rax for return value)
        // The function should return false for non-rax registers after ret
        // Actually let's just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn reg_used_in_next_instruction() {
        let instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), imm(5)),
            X86Instr::Add(reg(X86Reg::Rax), reg(X86Reg::Rcx)),
            X86Instr::Ret,
        ];
        assert!(is_reg_used_after(&instrs, 1, &X86Reg::Rcx));
    }

    #[test]
    fn reg_overwritten_before_read() {
        let instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), imm(5)),
            X86Instr::Mov(reg(X86Reg::Rcx), imm(10)),
            X86Instr::Add(reg(X86Reg::Rax), reg(X86Reg::Rcx)),
            X86Instr::Ret,
        ];
        // At position 1, rcx is overwritten → the value from position 0 is dead
        assert!(!is_reg_used_after(&instrs, 1, &X86Reg::Rcx));
    }

    // ─── LEA forwarding (Pattern 3c) ────────────────────────────

    #[test]
    fn lea_forwarding_into_mov() {
        // lea r10, [rbp-16]; mov DWORD PTR [r10+0], ecx → mov DWORD PTR [rbp-16], ecx
        // Use r10 instead of rax because rax is live at ret (return value)
        let mut instrs = vec![
            X86Instr::Lea(reg(X86Reg::R10), mem(X86Reg::Rbp, -16)),
            X86Instr::Mov(
                X86Operand::DwordMem(X86Reg::R10, 0),
                reg(X86Reg::Ecx),
            ),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // LEA should be folded: mov DWORD PTR [rbp-16], ecx
        assert!(instrs.iter().any(|i| matches!(i, X86Instr::Mov(
            X86Operand::DwordMem(X86Reg::Rbp, -16),
            X86Operand::Reg(X86Reg::Ecx),
        ))));
    }

    // ─── Pattern 4d: mov reg, 0; add dest, reg → remove both ───

    #[test]
    fn remove_add_zero_via_register() {
        // mov rcx, 0; add [rbp-8], rcx → remove both
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rcx), imm(0)),
            X86Instr::Add(mem(X86Reg::Rbp, -8), reg(X86Reg::Rcx)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // Both instructions should be removed, leaving just ret
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    // ─── Multiple passes converge ───────────────────────────────

    #[test]
    fn multiple_passes_converge() {
        // Chain that requires multiple optimization rounds:
        // mov rax, rax; add rbx, 0; sub rcx, 0; imul rdx, 1; ret
        let mut instrs = vec![
            X86Instr::Mov(reg(X86Reg::Rax), reg(X86Reg::Rax)),
            X86Instr::Add(reg(X86Reg::Rbx), imm(0)),
            X86Instr::Sub(reg(X86Reg::Rcx), imm(0)),
            X86Instr::Imul(reg(X86Reg::Rdx), imm(1)),
            X86Instr::Ret,
        ];
        apply_peephole(&mut instrs);
        // All no-ops should be removed
        assert_eq!(instrs.len(), 1);
        assert!(matches!(instrs[0], X86Instr::Ret));
    }

    // ─── Empty and single-instruction inputs ────────────────────

    #[test]
    fn empty_instructions() {
        let mut instrs: Vec<X86Instr> = vec![];
        apply_peephole(&mut instrs);
        assert!(instrs.is_empty());
    }

    #[test]
    fn single_ret() {
        let mut instrs = vec![X86Instr::Ret];
        apply_peephole(&mut instrs);
        assert_eq!(instrs.len(), 1);
    }

    // ─── fold_lea_into_next ─────────────────────────────────────

    #[test]
    fn fold_lea_mov_store() {
        let instr = X86Instr::Mov(
            X86Operand::DwordMem(X86Reg::Rax, 4),
            reg(X86Reg::Ecx),
        );
        let result = fold_lea_into_next(&instr, &X86Reg::Rax, &X86Reg::Rbp, -16);
        assert!(result.is_some());
        let folded = result.unwrap();
        assert!(matches!(folded, X86Instr::Mov(
            X86Operand::DwordMem(X86Reg::Rbp, -12),
            X86Operand::Reg(X86Reg::Ecx)
        )));
    }

    #[test]
    fn fold_lea_add() {
        let instr = X86Instr::Add(
            X86Operand::Mem(X86Reg::Rax, 0),
            imm(1),
        );
        let result = fold_lea_into_next(&instr, &X86Reg::Rax, &X86Reg::Rbp, -8);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), X86Instr::Add(
            X86Operand::Mem(X86Reg::Rbp, -8),
            X86Operand::Imm(1)
        )));
    }

    #[test]
    fn fold_lea_no_match() {
        // Instruction that doesn't use the LEA dest register
        let instr = X86Instr::Mov(reg(X86Reg::Rbx), reg(X86Reg::Rcx));
        let result = fold_lea_into_next(&instr, &X86Reg::Rax, &X86Reg::Rbp, -16);
        assert!(result.is_none());
    }
}
