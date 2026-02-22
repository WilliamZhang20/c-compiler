use ir::{Function, Instruction, Operand, VarId};
use std::collections::{HashMap, HashSet};

/// Copy propagation: replace uses of copies with their sources
///
/// Finds all simple copy instructions (x = y) and replaces uses of x with y
/// throughout the function. This simplifies the code and enables further optimizations.
pub fn copy_propagation(func: &mut Function) {
    // Count definitions per variable to detect Phi-resolved copies
    // (after phi removal, a phi with N preds becomes N copies to the same dest).
    // Only propagate when there is exactly one definition — anything else means
    // the variable carries different values on different control-flow paths.
    let mut def_count: HashMap<VarId, usize> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, .. } = inst {
                *def_count.entry(*dest).or_insert(0) += 1;
            }
        }
    }

    let mut copies: HashMap<VarId, Operand> = HashMap::new();

    // Collect copy instructions — only for singly-defined variables.
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, src } = inst {
                if def_count.get(dest).copied().unwrap_or(0) == 1 {
                    copies.insert(*dest, src.clone());
                }
            }
        }
    }

    // Resolve transitive copies: if x=y and y=z, then x=z
    // This is important for chains of copies that arise from SSA construction
    // Resolve in-place by following chains without cloning the entire map
    let keys: Vec<VarId> = copies.keys().copied().collect();
    for start in keys {
        let mut var = start;
        // Follow the chain: var -> src -> src's src -> ...
        loop {
            if let Some(Operand::Var(src_var)) = copies.get(&var) {
                let next = *src_var;
                if next == start {
                    break; // cycle
                }
                if copies.contains_key(&next) {
                    var = next;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        // If we followed a chain, update to the final target
        if var != start {
            if let Some(final_op) = copies.get(&var).cloned() {
                copies.insert(start, final_op);
            }
        }
    }

    // Track which variables are used after propagation
    let mut used_vars: HashSet<VarId> = HashSet::new();
    
    // Replace uses with copy sources and track variable usage
    for block in &mut func.blocks {
        // First, collect uses in phi nodes (they're special)
        for inst in &block.instructions {
            if let Instruction::Phi { preds, .. } = inst {
                for (_, var_id) in preds {
                    used_vars.insert(*var_id);
                }
            }
        }
        
        for inst in &mut block.instructions {
            match inst {
                Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => {
                    replace_operand(left, &copies);
                    replace_operand(right, &copies);
                    collect_used_var(left, &mut used_vars);
                    collect_used_var(right, &mut used_vars);
                }
                Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => {
                    replace_operand(src, &copies);
                    collect_used_var(src, &mut used_vars);
                }
                Instruction::Store { addr, src, .. } => {
                    replace_operand(addr, &copies);
                    replace_operand(src, &copies);
                    collect_used_var(addr, &mut used_vars);
                    collect_used_var(src, &mut used_vars);
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    replace_operand(base, &copies);
                    replace_operand(index, &copies);
                    collect_used_var(base, &mut used_vars);
                    collect_used_var(index, &mut used_vars);
                }
                Instruction::Call { args, .. } => {
                    for arg in args {
                        replace_operand(arg, &copies);
                        collect_used_var(arg, &mut used_vars);
                    }
                }
                Instruction::IndirectCall { func_ptr, args, .. } => {
                    replace_operand(func_ptr, &copies);
                    collect_used_var(func_ptr, &mut used_vars);
                    for arg in args {
                        replace_operand(arg, &copies);
                        collect_used_var(arg, &mut used_vars);
                    }
                }
                Instruction::Cast { src, .. } => {
                    replace_operand(src, &copies);
                    collect_used_var(src, &mut used_vars);
                }
                Instruction::Load { addr, .. } => {
                    replace_operand(addr, &copies);
                    collect_used_var(addr, &mut used_vars);
                }
                Instruction::Copy { src, .. } => {
                    // Also propagate through the source of copy instructions and
                    // track the (possibly updated) source as used so that DCE
                    // doesn't remove its definition.
                    replace_operand(src, &copies);
                    collect_used_var(src, &mut used_vars);
                }
                _ => {}
            }
        }

        // Also update terminators
        match &mut block.terminator {
            ir::Terminator::CondBr { cond, .. } => {
                replace_operand(cond, &copies);
                collect_used_var(cond, &mut used_vars);
            }
            ir::Terminator::Ret(Some(op)) => {
                replace_operand(op, &copies);
                collect_used_var(op, &mut used_vars);
            }
            _ => {}
        }
    }
    
    // Remove dead copy instructions (where the destination is not used)
    for block in &mut func.blocks {
        block.instructions.retain(|inst| {
            if let Instruction::Copy { dest, .. } = inst {
                // Keep the copy if the destination is used
                used_vars.contains(dest)
            } else {
                true
            }
        });
    }
}

fn replace_operand(op: &mut Operand, copies: &HashMap<VarId, Operand>) {
    if let Operand::Var(v) = op {
        if let Some(replacement) = copies.get(v) {
            *op = replacement.clone();
        }
    }
}

fn collect_used_var(op: &Operand, used: &mut HashSet<VarId>) {
    if let Operand::Var(v) = op {
        used.insert(*v);
    }
}
