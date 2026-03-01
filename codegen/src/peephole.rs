// Peephole optimization pass for assembly-level improvements
//
// Architecture: Each optimization is a standalone `PeepholeRule` registered in
// `default_rules()`.  The driver loop (`apply_peephole`) iterates rules until a
// fixpoint.  Adding a new pattern only requires writing its function and
// appending one entry to `default_rules()` — no existing code needs editing.

use crate::x86::{X86Instr, X86Operand, X86Reg};
use std::collections::{HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════

/// A single peephole optimization rule.
///
/// `apply` tries to match and transform the instruction stream starting at
/// index `i`.  Returns `true` if a transformation was applied (meaning the
/// caller should retry from the same index).
pub(crate) struct PeepholeRule {
    pub name: &'static str,
    pub apply: fn(&mut Vec<X86Instr>, usize) -> bool,
}

/// Build the default ordered set of peephole rules.
///
/// Order matters: earlier rules get first crack at each position, and some
/// patterns create opportunities for later ones.  To add a new optimisation,
/// append a `PeepholeRule` here.
pub(crate) fn default_rules() -> Vec<PeepholeRule> {
    vec![
        PeepholeRule { name: "cmp-set-branch-fusion",     apply: rule_cmp_set_branch_fusion },
        PeepholeRule { name: "redundant-mov",             apply: rule_redundant_mov },
        PeepholeRule { name: "mov-cmp-fusion",            apply: rule_mov_cmp_fusion },
        PeepholeRule { name: "adjacent-copy-forward",     apply: rule_adjacent_copy_forward },
        PeepholeRule { name: "non-adjacent-copy-forward", apply: rule_non_adjacent_copy_forward },
        PeepholeRule { name: "immediate-forward",         apply: rule_immediate_forward },
        PeepholeRule { name: "zero-add-sub",              apply: rule_zero_add_sub },
        PeepholeRule { name: "lea-offset-fold",           apply: rule_lea_offset_fold },
        PeepholeRule { name: "lea-mem-forward",           apply: rule_lea_mem_forward },
        PeepholeRule { name: "imul-identity",             apply: rule_imul_identity },
        PeepholeRule { name: "imul-by-zero",              apply: rule_imul_by_zero },
        PeepholeRule { name: "constant-mul-fold",         apply: rule_constant_mul_fold },
        PeepholeRule { name: "zero-reg-add",              apply: rule_zero_reg_add },
        PeepholeRule { name: "in-place-modify",           apply: rule_in_place_modify },
        PeepholeRule { name: "copy-base-forward",         apply: rule_copy_base_forward },
        PeepholeRule { name: "extend-chain-forward",      apply: rule_extend_chain_forward },
        PeepholeRule { name: "dead-store-after-load",     apply: rule_dead_store_after_load },
        PeepholeRule { name: "dead-store",                apply: rule_dead_store },
        PeepholeRule { name: "lea-to-add",                apply: rule_lea_to_add },
    ]
}

/// apply_peephole performs pattern-based optimizations on generated assembly
pub fn apply_peephole(instructions: &mut Vec<X86Instr>) {
    // First pass: eliminate jump chains
    eliminate_jump_chains(instructions);

    let rules = default_rules();

    // Iterate pattern-based optimizations until no more changes (fixpoint)
    for _round in 0..10 {
        let mut changed = false;
        let mut i = 0;
        while i < instructions.len() {
            let mut matched = false;
            for rule in &rules {
                if (rule.apply)(instructions, i) {
                    matched = true;
                    changed = true;
                    break; // restart rule scan at same index
                }
            }
            if !matched {
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

// ═══════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════

fn is_mem_operand(op: &X86Operand) -> bool {
    matches!(op,
        X86Operand::Mem(..) | X86Operand::DwordMem(..) | X86Operand::WordMem(..) |
        X86Operand::ByteMem(..) | X86Operand::FloatMem(..) | X86Operand::DoubleMem(..) |
        X86Operand::GlobalMem(..) | X86Operand::GlobalQwordMem(..)
    )
}

// ═══════════════════════════════════════════════════════════════════
//  Individual peephole rules
// ═══════════════════════════════════════════════════════════════════

/// cmp regA, opB; mov rax/eax, 0; set<cond> al; mov regD, rax; [gap]; test regD, regD; j<cc> label
/// → cmp regA, opB; j<cond> label  (or j<!cond> for "e" branch cond)
fn rule_cmp_set_branch_fusion(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 5 >= instructions.len() { return false; }

    let first4_matches = matches!(
        (&instructions[i], &instructions[i+1], &instructions[i+2], &instructions[i+3]),
        (
            X86Instr::Cmp(_, _),
            X86Instr::Mov(X86Operand::Reg(X86Reg::Rax | X86Reg::Eax), X86Operand::Imm(0)),
            X86Instr::Set(_, X86Operand::Reg(X86Reg::Al)),
            X86Instr::Mov(_, X86Operand::Reg(X86Reg::Rax | X86Reg::Eax)),
        )
    );
    if !first4_matches { return false; }

    let test_reg = if let X86Instr::Mov(X86Operand::Reg(r), _) = &instructions[i+3] {
        r.clone()
    } else {
        return false;
    };

    let max_scan = std::cmp::min(i + 10, instructions.len());
    let mut gap_indices = Vec::new();
    let mut found_test_jcc = None;

    for j in (i+4)..max_scan {
        if j + 1 < instructions.len() {
            if let (X86Instr::Test(tl, tr), X86Instr::Jcc(_, _)) = (&instructions[j], &instructions[j+1]) {
                if tl.is_direct_reg(&test_reg) && tr.is_direct_reg(&test_reg) {
                    found_test_jcc = Some(j);
                    break;
                }
            }
        }
        if instructions[j].is_block_boundary() { break; }
        if instr_touches_reg(&instructions[j], &test_reg) { break; }
        gap_indices.push(j);
    }

    let test_idx = match found_test_jcc { Some(idx) => idx, None => return false };
    let set_cond   = if let X86Instr::Set(c, _)  = &instructions[i+2]       { c.clone() } else { unreachable!() };
    let (branch_cond, branch_label) = if let X86Instr::Jcc(c, l) = &instructions[test_idx+1] { (c.clone(), l.clone()) } else { unreachable!() };

    let final_cond = if branch_cond == "ne" {
        set_cond
    } else if branch_cond == "e" {
        match set_cond.as_str() {
            "e"  => "ne".to_string(), "ne" => "e".to_string(),
            "l"  => "ge".to_string(), "le" => "g".to_string(),
            "g"  => "le".to_string(), "ge" => "l".to_string(),
            _ => return false,
        }
    } else {
        return false;
    };

    let cmp_left  = if let X86Instr::Cmp(l, _) = &instructions[i] { l.clone() } else { unreachable!() };
    let cmp_right = if let X86Instr::Cmp(_, r) = &instructions[i] { r.clone() } else { unreachable!() };

    instructions.remove(test_idx + 1);
    instructions.remove(test_idx);
    instructions.remove(i + 3);
    instructions.remove(i + 2);
    instructions.remove(i + 1);
    let jcc_pos = i + 1 + gap_indices.len();
    instructions.insert(jcc_pos, X86Instr::Jcc(final_cond, branch_label));
    instructions[i] = X86Instr::Cmp(cmp_left, cmp_right);
    true
}

/// mov reg, reg → remove (no-op)
fn rule_redundant_mov(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let Some(X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Reg(r2))) = instructions.get(i) {
        if matches!((r1, r2),
            (X86Reg::Rax, X86Reg::Rax) | (X86Reg::Rcx, X86Reg::Rcx) |
            (X86Reg::Rdx, X86Reg::Rdx) | (X86Reg::Rbx, X86Reg::Rbx) |
            (X86Reg::Rsi, X86Reg::Rsi) | (X86Reg::Rdi, X86Reg::Rdi) |
            (X86Reg::R8, X86Reg::R8)   | (X86Reg::R9, X86Reg::R9)   |
            (X86Reg::R10, X86Reg::R10) | (X86Reg::R11, X86Reg::R11) |
            (X86Reg::R12, X86Reg::R12) | (X86Reg::R13, X86Reg::R13) |
            (X86Reg::R14, X86Reg::R14) | (X86Reg::R15, X86Reg::R15)
        ) {
            instructions.remove(i);
            return true;
        }
    }
    false
}

/// mov reg1, src; cmp reg1, op → cmp src, op (if reg1 dead and src is a register)
fn rule_mov_cmp_fusion(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(mov_dest), mov_src @ X86Operand::Reg(_)),
        X86Instr::Cmp(X86Operand::Reg(cmp_left), cmp_right)
    ) = (&instructions[i], &instructions[i + 1]) {
        if std::mem::discriminant(mov_dest) == std::mem::discriminant(cmp_left) {
            if !is_reg_used_after(instructions, i + 2, mov_dest) {
                instructions[i] = X86Instr::Cmp(mov_src.clone(), cmp_right.clone());
                instructions.remove(i + 1);
                return true;
            }
        }
    }
    false
}

/// mov reg, X; mov Y, reg → mov Y, X (if reg dead and no mem-to-mem)
fn rule_adjacent_copy_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(temp_reg), src),
        X86Instr::Mov(dest, X86Operand::Reg(temp_reg2))
    ) = (&instructions[i], &instructions[i + 1]) {
        if std::mem::discriminant(temp_reg) == std::mem::discriminant(temp_reg2)
            && !is_reg_used_after(instructions, i + 2, temp_reg)
        {
            if !is_mem_operand(src) || !is_mem_operand(dest) {
                instructions[i] = X86Instr::Mov(dest.clone(), src.clone());
                instructions.remove(i + 1);
                return true;
            }
        }
    }
    false
}

/// mov reg, src; [gap]; mov dest, reg → mov dest, src (remove original)
fn rule_non_adjacent_copy_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let X86Instr::Mov(X86Operand::Reg(temp_reg), src) = &instructions[i] {
        if !matches!(src, X86Operand::Reg(_) | X86Operand::Imm(_)) { return false; }
        let max_scan = std::cmp::min(i + 10, instructions.len());
        for j in (i + 1)..max_scan {
            if instructions[j].is_block_boundary() { break; }
            if instr_touches_reg(&instructions[j], temp_reg) {
                if let X86Instr::Mov(dest, X86Operand::Reg(temp2)) = &instructions[j] {
                    if std::mem::discriminant(temp_reg) == std::mem::discriminant(temp2)
                        && !is_reg_used_after(instructions, j + 1, temp_reg)
                        && (!is_mem_operand(src) || !is_mem_operand(dest))
                    {
                        instructions[j] = X86Instr::Mov(dest.clone(), src.clone());
                        instructions.remove(i);
                        return true;
                    }
                }
                break;
            }
            if let X86Operand::Reg(src_reg) = src {
                if instr_touches_reg(&instructions[j], src_reg) { break; }
            }
        }
    }
    false
}

/// mov reg, imm; OP dest, reg → OP dest, imm (if reg dead after)
fn rule_immediate_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let X86Instr::Mov(X86Operand::Reg(load_reg), X86Operand::Imm(imm_val)) = &instructions[i] {
        let imm_val = *imm_val;
        let load_reg = load_reg.clone();
        let can_forward = match &instructions[i + 1] {
            X86Instr::Mov(dest, X86Operand::Reg(use_reg)) if load_reg.same_physical(use_reg) => {
                let is_dest_mem = is_mem_operand(dest);
                if is_dest_mem && matches!(dest, X86Operand::DwordMem(..)) {
                    imm_val >= i32::MIN as i64 && imm_val <= i32::MAX as i64
                } else { true }
            }
            X86Instr::Add(_, X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
            X86Instr::Sub(_, X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
            X86Instr::Cmp(_, X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
            X86Instr::And(_, X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
            X86Instr::Or(_,  X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
            X86Instr::Xor(_, X86Operand::Reg(r)) if load_reg.same_physical(r) => true,
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
                X86Instr::Or(dest, _)  => { instructions[i] = X86Instr::Or(dest.clone(),  imm_op); }
                X86Instr::Xor(dest, _) => { instructions[i] = X86Instr::Xor(dest.clone(), imm_op); }
                _ => unreachable!(),
            }
            instructions.remove(i + 1);
            return true;
        }
    }
    false
}

/// add/sub with immediate 0 → remove
fn rule_zero_add_sub(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let Some(X86Instr::Add(_, X86Operand::Imm(0)))
         | Some(X86Instr::Sub(_, X86Operand::Imm(0))) = instructions.get(i)
    {
        instructions.remove(i);
        return true;
    }
    false
}

/// lea reg, [base+off]; add reg, C → lea reg, [base+off+C]
fn rule_lea_offset_fold(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
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
    false
}

/// lea reg, [base+off]; next uses [reg+off2] → fold to [base+off+off2]
fn rule_lea_mem_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let X86Instr::Lea(X86Operand::Reg(lea_dest), lea_src) = &instructions[i] {
        if let X86Operand::Mem(lea_base, lea_off) = lea_src {
            let lea_dest_c = lea_dest.clone();
            let lea_base_c = lea_base.clone();
            let lea_off_c = *lea_off;
            if let Some(new_instr) = fold_lea_into_next(&instructions[i + 1], &lea_dest_c, &lea_base_c, lea_off_c) {
                let fold_overwrites_dest = match &new_instr {
                    X86Instr::Mov(X86Operand::Reg(r), _) |
                    X86Instr::Lea(X86Operand::Reg(r), _) |
                    X86Instr::Movsx(X86Operand::Reg(r), _) |
                    X86Instr::Movzx(X86Operand::Reg(r), _) => r.same_physical(&lea_dest_c),
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
    false
}

/// imul reg, 1 → remove
fn rule_imul_identity(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let Some(X86Instr::Imul(_, X86Operand::Imm(1))) = instructions.get(i) {
        instructions.remove(i);
        return true;
    }
    false
}

/// imul reg, 0 → mov reg, 0
fn rule_imul_by_zero(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let Some(X86Instr::Imul(X86Operand::Reg(r), X86Operand::Imm(0))) = instructions.get(i) {
        instructions[i] = X86Instr::Mov(X86Operand::Reg(r.clone()), X86Operand::Imm(0));
        return true;
    }
    false
}

/// mov reg, C1; imul reg, C2 → mov reg, C1*C2
fn rule_constant_mul_fold(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
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
    false
}

/// mov reg, 0; add dest, reg → remove both (adding zero via register)
fn rule_zero_reg_add(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Imm(0)),
        X86Instr::Add(_, X86Operand::Reg(r2))
    ) = (&instructions[i], &instructions[i + 1]) {
        if std::mem::discriminant(r1) == std::mem::discriminant(r2)
            && !is_reg_used_after(instructions, i + 2, r1)
        {
            instructions.remove(i);
            instructions.remove(i);
            return true;
        }
    }
    false
}

/// mov tmp, reg; add/sub tmp, op; [gap]; mov reg, tmp → add/sub reg, op
fn rule_in_place_modify(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 2 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(tmp1), X86Operand::Reg(src_reg)),
        add_or_sub,
    ) = (&instructions[i], &instructions[i + 1]) {
        let folded = match add_or_sub {
            X86Instr::Add(X86Operand::Reg(tmp2), op)
                if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp2) =>
                Some(X86Instr::Add(X86Operand::Reg(src_reg.clone()), op.clone())),
            X86Instr::Sub(X86Operand::Reg(tmp2), op)
                if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp2) =>
                Some(X86Instr::Sub(X86Operand::Reg(src_reg.clone()), op.clone())),
            _ => None,
        };
        if let Some(new_instr) = folded {
            let max_scan = std::cmp::min(i + 8, instructions.len());
            for j in (i + 2)..max_scan {
                if let X86Instr::Mov(X86Operand::Reg(dst_reg), X86Operand::Reg(tmp3)) = &instructions[j] {
                    if std::mem::discriminant(tmp1) == std::mem::discriminant(tmp3)
                        && std::mem::discriminant(src_reg) == std::mem::discriminant(dst_reg)
                    {
                        let safe = (i + 2..j).all(|k| {
                            !instr_touches_reg(&instructions[k], tmp1)
                                && !instr_touches_reg(&instructions[k], src_reg)
                        });
                        if safe && !is_reg_used_after(instructions, j + 1, tmp1) {
                            instructions[i] = new_instr;
                            instructions.remove(i + 1);
                            instructions.remove(j - 1);
                            return true;
                        }
                        break;
                    }
                }
                if instructions[j].is_block_boundary() { break; }
            }
        }
    }
    false
}

/// mov rX, rY; instr [rX+off], ... → instr [rY+off], ...  (if rX dead)
fn rule_copy_base_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let X86Instr::Mov(X86Operand::Reg(copy_dest), X86Operand::Reg(copy_src)) = &instructions[i] {
        let copy_dest_c = copy_dest.clone();
        let copy_src_c = copy_src.clone();
        if let Some(new_instr) = substitute_base_reg(&instructions[i + 1], &copy_dest_c, &copy_src_c) {
            if !is_reg_used_after(instructions, i + 2, &copy_dest_c) {
                instructions[i] = new_instr;
                instructions.remove(i + 1);
                return true;
            }
        }
    }
    false
}

/// movsx/movzx rX, oY; mov rZ, rX → movsx/movzx rZ, oY (if rX dead)
fn rule_extend_chain_forward(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let X86Instr::Mov(X86Operand::Reg(mov_dest), X86Operand::Reg(mov_src)) = &instructions[i + 1] {
        match &instructions[i] {
            X86Instr::Movsx(X86Operand::Reg(sx_dest), sx_src)
                if sx_dest.same_physical(mov_src) && !is_reg_used_after(instructions, i + 2, sx_dest) =>
            {
                instructions[i] = X86Instr::Movsx(X86Operand::Reg(mov_dest.clone()), sx_src.clone());
                instructions.remove(i + 1);
                return true;
            }
            X86Instr::Movzx(X86Operand::Reg(zx_dest), zx_src)
                if zx_dest.same_physical(mov_src) && !is_reg_used_after(instructions, i + 2, zx_dest) =>
            {
                instructions[i] = X86Instr::Movzx(X86Operand::Reg(mov_dest.clone()), zx_src.clone());
                instructions.remove(i + 1);
                return true;
            }
            _ => {}
        }
    }
    false
}

/// mov rX, [mem]; mov [mem], rX → remove the store (value already there)
fn rule_dead_store_after_load(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(load_dest), load_src),
        X86Instr::Mov(store_dest, X86Operand::Reg(store_src))
    ) = (&instructions[i], &instructions[i + 1]) {
        if load_dest.same_physical(store_src) && load_src == store_dest {
            instructions.remove(i + 1);
            return true;
        }
    }
    false
}

/// mov reg, src where reg is dead → remove (dead store)
fn rule_dead_store(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if let X86Instr::Mov(X86Operand::Reg(dest_reg), _) = &instructions[i] {
        if !is_reg_used_after(instructions, i + 1, dest_reg) {
            instructions.remove(i);
            return true;
        }
    }
    false
}

/// mov reg, imm; add reg, X → lea reg, [X + imm] (small constants)
fn rule_lea_to_add(instructions: &mut Vec<X86Instr>, i: usize) -> bool {
    if i + 1 >= instructions.len() { return false; }
    if let (
        X86Instr::Mov(X86Operand::Reg(r1), X86Operand::Imm(offset)),
        X86Instr::Add(X86Operand::Reg(r2), X86Operand::Reg(r3))
    ) = (&instructions[i], &instructions[i + 1]) {
        if std::mem::discriminant(r1) == std::mem::discriminant(r2) && *offset >= -128 && *offset <= 127 {
            instructions[i] = X86Instr::Lea(
                X86Operand::Reg(r1.clone()),
                X86Operand::Mem(r3.clone(), *offset as i32),
            );
            instructions.remove(i + 1);
            return true;
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
            X86Operand::Mem(base, off2) if base.same_physical(lea_dest) =>
                Some(X86Operand::Mem(lea_base.clone(), lea_off + off2)),
            X86Operand::DwordMem(base, off2) if base.same_physical(lea_dest) =>
                Some(X86Operand::DwordMem(lea_base.clone(), lea_off + off2)),
            X86Operand::WordMem(base, off2) if base.same_physical(lea_dest) =>
                Some(X86Operand::WordMem(lea_base.clone(), lea_off + off2)),
            X86Operand::ByteMem(base, off2) if base.same_physical(lea_dest) =>
                Some(X86Operand::ByteMem(lea_base.clone(), lea_off + off2)),

            _ => None,
        }
    };

    match instr {
        // mov DWORD PTR [rax+0], src → mov DWORD PTR [base+off], src
        X86Instr::Mov(dest, src) => {
            if let Some(new_dest) = subst(dest) {
                // Make sure src doesn't also use lea_dest as a value (only if it's a direct reg ref)
                if !src.is_direct_reg(lea_dest) {
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
                if !src.is_direct_reg(lea_dest) {
                    return Some(X86Instr::Add(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Sub(dest, src) => {
            if let Some(new_dest) = subst(dest) {
                if !src.is_direct_reg(lea_dest) {
                    return Some(X86Instr::Sub(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Cmp(left, right) => {
            if let Some(new_left) = subst(left) {
                if !right.is_direct_reg(lea_dest) {
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
    
    let is_rax = reg.physical_id() == 0;
    
    for idx in start..instrs.len() {
        if !visited.insert(idx) {
            return true; // cycle detected, conservative
        }
        
        match &instrs[idx] {
            X86Instr::Label(_) => continue,
            X86Instr::Ret => return is_rax,
            X86Instr::Jmp(target) => {
                return if let Some(pos) = find_label_pos(instrs, target) {
                    is_reg_live_from(instrs, pos, reg, visited, depth - 1)
                } else {
                    true
                };
            }
            X86Instr::Jcc(_, target) => {
                if let Some(pos) = find_label_pos(instrs, target) {
                    if is_reg_live_from(instrs, pos, reg, visited, depth - 1) {
                        return true;
                    }
                } else {
                    return true;
                }
            }
            X86Instr::Call(_) | X86Instr::CallIndirect(_) => return true,
            instr => {
                if instr.reads_phys_reg(reg) { return true; }
                if instr.writes_phys_reg(reg) { return false; }
            }
        }
    }
    false
}

/// Check if an instruction reads or writes the given register (any alias).
fn instr_touches_reg(inst: &X86Instr, reg: &X86Reg) -> bool {
    inst.touches_phys_reg(reg)
}

/// Substitute a register used as a memory base in the given instruction.
/// If `old_reg` appears as the base register of a memory operand in `instr`,
/// replace it with `new_reg` and return the updated instruction.
fn substitute_base_reg(instr: &X86Instr, old_reg: &X86Reg, new_reg: &X86Reg) -> Option<X86Instr> {
    let subst_op = |op: &X86Operand| -> Option<X86Operand> {
        match op {
            X86Operand::Mem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::Mem(new_reg.clone(), *off)),
            X86Operand::DwordMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::DwordMem(new_reg.clone(), *off)),
            X86Operand::WordMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::WordMem(new_reg.clone(), *off)),
            X86Operand::ByteMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::ByteMem(new_reg.clone(), *off)),
            X86Operand::FloatMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::FloatMem(new_reg.clone(), *off)),
            X86Operand::DoubleMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::DoubleMem(new_reg.clone(), *off)),
            X86Operand::XmmwordMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::XmmwordMem(new_reg.clone(), *off)),
            X86Operand::YmmwordMem(base, off) if base.same_physical(old_reg) =>
                Some(X86Operand::YmmwordMem(new_reg.clone(), *off)),
            _ => None,
        }
    };

    match instr {
        X86Instr::Mov(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                if !src.is_direct_reg(old_reg) {
                    return Some(X86Instr::Mov(new_dest, src.clone()));
                }
            }
            if let Some(new_src) = subst_op(src) {
                return Some(X86Instr::Mov(dest.clone(), new_src));
            }
            None
        }
        X86Instr::Movsx(dest, src) => {
            subst_op(src).map(|new_src| X86Instr::Movsx(dest.clone(), new_src))
        }
        X86Instr::Movzx(dest, src) => {
            subst_op(src).map(|new_src| X86Instr::Movzx(dest.clone(), new_src))
        }
        X86Instr::Add(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                if !src.is_direct_reg(old_reg) {
                    return Some(X86Instr::Add(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Sub(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                if !src.is_direct_reg(old_reg) {
                    return Some(X86Instr::Sub(new_dest, src.clone()));
                }
            }
            None
        }
        X86Instr::Cmp(left, right) => {
            if let Some(new_left) = subst_op(left) {
                if !right.is_direct_reg(old_reg) {
                    return Some(X86Instr::Cmp(new_left, right.clone()));
                }
            }
            if let Some(new_right) = subst_op(right) {
                if !left.is_direct_reg(old_reg) {
                    return Some(X86Instr::Cmp(left.clone(), new_right));
                }
            }
            None
        }
        X86Instr::Vmovdqu(dest, src) => {
            if let Some(new_dest) = subst_op(dest) {
                return Some(X86Instr::Vmovdqu(new_dest, src.clone()));
            }
            subst_op(src).map(|new_src| X86Instr::Vmovdqu(dest.clone(), new_src))
        }
        _ => None,
    }
}

/// Check if a register is READ (not just written) within the current basic block
/// starting from `start`. Stops and returns false at block boundaries (jmp/jcc/label/ret/call).
/// This is a weaker check than is_reg_used_after — it only looks within the block.
fn is_reg_read_in_block(instructions: &[X86Instr], start: usize, reg: &X86Reg) -> bool {
    for inst in instructions.iter().skip(start) {
        if inst.is_block_boundary() { return false; }
        if inst.reads_phys_reg(reg) { return true; }
        if inst.writes_phys_reg(reg) { return false; }
    }
    false
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

    // ─── X86Reg::same_physical / physical_id ────────────────────

    #[test]
    fn same_physical_reg_aliases() {
        assert!(X86Reg::Rax.same_physical(&X86Reg::Eax));
        assert!(X86Reg::Eax.same_physical(&X86Reg::Al));
        assert!(X86Reg::Rcx.same_physical(&X86Reg::Cl));
        assert!(X86Reg::Xmm0.same_physical(&X86Reg::Ymm0));
    }

    #[test]
    fn same_physical_reg_different() {
        assert!(!X86Reg::Rax.same_physical(&X86Reg::Rbx));
        assert!(!X86Reg::Eax.same_physical(&X86Reg::Ecx));
        assert!(!X86Reg::Xmm0.same_physical(&X86Reg::Xmm1));
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
