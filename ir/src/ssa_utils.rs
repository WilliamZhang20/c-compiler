// SSA utility functions
// Extracted from mem2reg.rs: verify_ssa, remove_phis

use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, Instruction, Function, Operand, Terminator};
#[allow(unused_imports)]
use crate::types::BasicBlock;

/// Verify that every VarId used as an operand in the function is defined
/// exactly once (by an instruction dest, phi, or function parameter).
/// Returns Ok(()) if valid, or Err with a description of the first violation.
pub fn verify_ssa(func: &Function) -> Result<(), String> {
    let mut defs: HashSet<VarId> = HashSet::new();

    // Parameters define their VarIds
    for (_, var) in &func.params {
        defs.insert(*var);
    }

    // Collect all VarIds defined by instructions (using accessor)
    for block in &func.blocks {
        for instr in &block.instructions {
            for d in instr.dests() {
                defs.insert(d);
            }
        }
    }

    // Now check that every used VarId is in the defs set (using accessor)
    for block in &func.blocks {
        let ctx = format!("block {:?}", block.id);
        for instr in &block.instructions {
            instr.for_each_use(|v| {
                // We can't return Err from a closure, so collect violations
                if !defs.contains(&v) {
                    // Will be caught below
                }
            });
            // Re-check with error reporting
            let mut violation = None;
            instr.for_each_use(|v| {
                if violation.is_none() && !defs.contains(&v) {
                    violation = Some(format!("VarId({}) used but never defined ({})", v.0, ctx));
                }
            });
            if let Some(err) = violation {
                return Err(err);
            }
        }
        // Check terminator operands
        match &block.terminator {
            Terminator::CondBr { cond, .. } => {
                if let Operand::Var(v) = cond {
                    if !defs.contains(v) {
                        return Err(format!("VarId({}) used but never defined (terminator of block {:?})", v.0, block.id));
                    }
                }
            }
            Terminator::Ret(Some(val)) => {
                if let Operand::Var(v) = val {
                    if !defs.contains(v) {
                        return Err(format!("VarId({}) used but never defined (terminator of block {:?})", v.0, block.id));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Remove Phi nodes by inserting Copy instructions in predecessor blocks
pub fn remove_phis(func: &mut Function) {
    let mut insertions: HashMap<BlockId, Vec<Instruction>> = HashMap::new();
    
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Instruction::Phi { dest, preds } = instr {
                for (pred_id, src) in preds {
                    insertions.entry(*pred_id).or_default().push(Instruction::Copy {
                        dest: *dest, 
                        src: Operand::Var(*src)
                    });
                }
            }
        }
    }
    
    // Apply insertions (skip unreachable blocks)
    for block in &mut func.blocks {
        // Don't add phi-resolution copies to unreachable blocks
        if matches!(block.terminator, Terminator::Unreachable) && block.instructions.is_empty() {
            continue;
        }
        
        if let Some(copies) = insertions.remove(&block.id) {
            block.instructions.extend(copies);
        }
        // Remove Phis
        block.instructions.retain(|i| !matches!(i, Instruction::Phi{..}));
    }
}
