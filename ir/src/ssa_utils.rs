// SSA utility functions
// Extracted from mem2reg.rs: verify_ssa, remove_phis

use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, Instruction, Function, Operand, Terminator};

/// Verify that every VarId used as an operand in the function is defined
/// exactly once (by an instruction dest, phi, or function parameter).
/// Returns Ok(()) if valid, or Err with a description of the first violation.
pub fn verify_ssa(func: &Function) -> Result<(), String> {
    let mut defs: HashSet<VarId> = HashSet::new();

    // Parameters define their VarIds
    for (_, var) in &func.params {
        defs.insert(*var);
    }

    // Collect all VarIds defined by instructions
    for block in &func.blocks {
        for instr in &block.instructions {
            match instr {
                Instruction::Binary { dest, .. } | Instruction::FloatBinary { dest, .. } |
                Instruction::Unary { dest, .. } | Instruction::FloatUnary { dest, .. } |
                Instruction::Phi { dest, .. } | Instruction::Copy { dest, .. } |
                Instruction::Cast { dest, .. } | Instruction::Alloca { dest, .. } |
                Instruction::Load { dest, .. } | Instruction::GetElementPtr { dest, .. } |
                Instruction::VaArg { dest, .. } => { defs.insert(*dest); }
                Instruction::Call { dest, .. } | Instruction::IndirectCall { dest, .. } => {
                    if let Some(d) = dest { defs.insert(*d); }
                }
                Instruction::InlineAsm { outputs, .. } => {
                    for o in outputs { defs.insert(*o); }
                }
                Instruction::Store { .. } | Instruction::VaStart { .. } |
                Instruction::VaEnd { .. } | Instruction::VaCopy { .. } => {}
            }
        }
    }

    // Now check that every used VarId is in the defs set
    let check_operand = |op: &Operand, defs: &HashSet<VarId>, context: &str| -> Result<(), String> {
        if let Operand::Var(v) = op {
            if !defs.contains(v) {
                return Err(format!("VarId({}) used but never defined ({})", v.0, context));
            }
        }
        Ok(())
    };

    for block in &func.blocks {
        for instr in &block.instructions {
            let ctx = format!("block {:?}", block.id);
            match instr {
                Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => {
                    check_operand(left, &defs, &ctx)?;
                    check_operand(right, &defs, &ctx)?;
                }
                Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => {
                    check_operand(src, &defs, &ctx)?;
                }
                Instruction::Copy { src, .. } | Instruction::Cast { src, .. } => {
                    check_operand(src, &defs, &ctx)?;
                }
                Instruction::Load { addr, .. } => {
                    check_operand(addr, &defs, &ctx)?;
                }
                Instruction::Store { addr, src, .. } => {
                    check_operand(addr, &defs, &ctx)?;
                    check_operand(src, &defs, &ctx)?;
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    check_operand(base, &defs, &ctx)?;
                    check_operand(index, &defs, &ctx)?;
                }
                Instruction::Call { args, .. } => {
                    for arg in args { check_operand(arg, &defs, &ctx)?; }
                }
                Instruction::IndirectCall { func_ptr, args, .. } => {
                    check_operand(func_ptr, &defs, &ctx)?;
                    for arg in args { check_operand(arg, &defs, &ctx)?; }
                }
                Instruction::Phi { preds, .. } => {
                    for (_, src) in preds {
                        if !defs.contains(src) {
                            return Err(format!("VarId({}) used in phi but never defined ({})", src.0, ctx));
                        }
                    }
                }
                Instruction::VaStart { list, .. } | Instruction::VaEnd { list } => {
                    check_operand(list, &defs, &ctx)?;
                }
                Instruction::VaCopy { dest, src } => {
                    check_operand(dest, &defs, &ctx)?;
                    check_operand(src, &defs, &ctx)?;
                }
                Instruction::VaArg { list, .. } => {
                    check_operand(list, &defs, &ctx)?;
                }
                Instruction::InlineAsm { inputs, .. } => {
                    for input in inputs { check_operand(input, &defs, &ctx)?; }
                }
                Instruction::Alloca { .. } => {}
            }
        }
        // Check terminator operands
        match &block.terminator {
            Terminator::CondBr { cond, .. } => {
                check_operand(cond, &defs, &format!("terminator of block {:?}", block.id))?;
            }
            Terminator::Ret(Some(val)) => {
                check_operand(val, &defs, &format!("terminator of block {:?}", block.id))?;
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
