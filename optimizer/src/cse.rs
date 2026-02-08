use ir::{Function, Instruction, Operand, VarId};
use std::collections::HashMap;

/// Common subexpression elimination: eliminate redundant calculations
///
/// Finds expressions that are computed multiple times with the same operands
/// and reuses the first computation instead of recalculating.
pub fn common_subexpression_elimination(func: &mut Function) {
    // Map from expression to the variable holding the result
    #[derive(Hash, Eq, PartialEq, Clone)]
    enum ExprKey {
        Binary(String, String, String), // (op, left_str, right_str)
        Unary(String, String),           // (op, src_str)
    }

    let mut expr_map: HashMap<ExprKey, VarId> = HashMap::new();
    let mut var_replacements: HashMap<VarId, VarId> = HashMap::new();

    // Find duplicate expressions
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Binary {
                    dest,
                    op,
                    left,
                    right,
                } => {
                    let key = ExprKey::Binary(
                        format!("{:?}", op),
                        format!("{:?}", left),
                        format!("{:?}", right),
                    );

                    if let Some(&existing_var) = expr_map.get(&key) {
                        // Found a duplicate! Mark for replacement
                        var_replacements.insert(*dest, existing_var);
                    } else {
                        expr_map.insert(key, *dest);
                    }
                }
                Instruction::Unary { dest, op, src } => {
                    let key = ExprKey::Unary(format!("{:?}", op), format!("{:?}", src));

                    if let Some(&existing_var) = expr_map.get(&key) {
                        var_replacements.insert(*dest, existing_var);
                    } else {
                        expr_map.insert(key, *dest);
                    }
                }
                _ => {}
            }
        }
    }

    // Replace all uses of eliminated variables
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            replace_in_instruction(inst, &var_replacements);
        }

        // Update terminators
        match &mut block.terminator {
            ir::Terminator::Ret(Some(op)) => {
                replace_in_operand(op, &var_replacements);
            }
            ir::Terminator::CondBr { cond, .. } => {
                replace_in_operand(cond, &var_replacements);
            }
            _ => {}
        }
    }
}

fn replace_in_instruction(inst: &mut Instruction, replacements: &HashMap<VarId, VarId>) {
    match inst {
        Instruction::Binary { left, right, .. } => {
            replace_in_operand(left, replacements);
            replace_in_operand(right, replacements);
        }
        Instruction::Unary { src, .. } => {
            replace_in_operand(src, replacements);
        }
        Instruction::Store { addr, src, .. } => {
            replace_in_operand(addr, replacements);
            replace_in_operand(src, replacements);
        }
        Instruction::Copy { src, .. } => {
            replace_in_operand(src, replacements);
        }
        Instruction::GetElementPtr { base, index, .. } => {
            replace_in_operand(base, replacements);
            replace_in_operand(index, replacements);
        }
        Instruction::Call { args, .. } => {
            for arg in args {
                replace_in_operand(arg, replacements);
            }
        }
        Instruction::IndirectCall { func_ptr, args, .. } => {
            replace_in_operand(func_ptr, replacements);
            for arg in args {
                replace_in_operand(arg, replacements);
            }
        }
        _ => {}
    }
}

fn replace_in_operand(op: &mut Operand, replacements: &HashMap<VarId, VarId>) {
    if let Operand::Var(v) = op {
        if let Some(&replacement) = replacements.get(v) {
            *op = Operand::Var(replacement);
        }
    }
}
