use ir::{Function, Instruction, Operand, VarId};
use std::collections::HashMap;

/// Copy propagation: replace uses of copies with their sources
///
/// Finds all simple copy instructions (x = y) and replaces uses of x with y
/// throughout the function. This simplifies the code and enables further optimizations.
pub fn copy_propagation(func: &mut Function) {
    let mut copies: HashMap<VarId, Operand> = HashMap::new();

    // Collect all copy instructions
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, src } = inst {
                copies.insert(*dest, src.clone());
            }
        }
    }

    // Replace uses with copy sources
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            match inst {
                Instruction::Binary { left, right, .. } => {
                    replace_operand(left, &copies);
                    replace_operand(right, &copies);
                }
                Instruction::Unary { src, .. } => {
                    replace_operand(src, &copies);
                }
                Instruction::Store { addr, src } => {
                    replace_operand(addr, &copies);
                    replace_operand(src, &copies);
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    replace_operand(base, &copies);
                    replace_operand(index, &copies);
                }
                Instruction::Call { args, .. } => {
                    for arg in args {
                        replace_operand(arg, &copies);
                    }
                }
                Instruction::IndirectCall { func_ptr, args, .. } => {
                    replace_operand(func_ptr, &copies);
                    for arg in args {
                        replace_operand(arg, &copies);
                    }
                }
                _ => {}
            }
        }

        // Also update terminators
        match &mut block.terminator {
            ir::Terminator::CondBr { cond, .. } => {
                replace_operand(cond, &copies);
            }
            ir::Terminator::Ret(Some(op)) => {
                replace_operand(op, &copies);
            }
            _ => {}
        }
    }
}

fn replace_operand(op: &mut Operand, copies: &HashMap<VarId, Operand>) {
    if let Operand::Var(v) = op {
        if let Some(replacement) = copies.get(v) {
            *op = replacement.clone();
        }
    }
}
