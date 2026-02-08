use ir::{Function, Instruction, Operand};
use model::BinaryOp;
use crate::utils::{is_power_of_two, log2};

/// Strength reduction: replace expensive operations with cheaper equivalents
///
/// Examples:
/// - x * (power of 2) → x << log2(power)
/// - x / (power of 2) → x >> log2(power)
/// - x % (power of 2) → x & (power - 1)
pub fn strength_reduce_function(func: &mut Function) {
    for block in &mut func.blocks {
        let mut new_instructions = Vec::new();

        for inst in block.instructions.drain(..) {
            match inst {
                Instruction::Binary {
                    dest,
                    op,
                    left,
                    right,
                } => {
                    if let Some(reduced) = try_reduce_binary(&op, &left, &right, dest) {
                        new_instructions.push(reduced);
                    } else {
                        new_instructions.push(Instruction::Binary {
                            dest,
                            op,
                            left,
                            right,
                        });
                    }
                }
                other => new_instructions.push(other),
            }
        }

        block.instructions = new_instructions;
    }
}

fn try_reduce_binary(
    op: &BinaryOp,
    left: &Operand,
    right: &Operand,
    dest: ir::VarId,
) -> Option<Instruction> {
    match op {
        BinaryOp::Mul => reduce_mul(left, right, dest),
        BinaryOp::Div => reduce_div(left, right, dest),
        BinaryOp::Mod => reduce_mod(left, right, dest),
        _ => None,
    }
}

fn reduce_mul(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x * (power of 2) → x << log2(power)
    if let Operand::Constant(c) = right {
        if is_power_of_two(*c) {
            return Some(Instruction::Binary {
                dest,
                op: BinaryOp::ShiftLeft,
                left: left.clone(),
                right: Operand::Constant(log2(*c)),
            });
        }
    }
    // (power of 2) * x → x << log2(power)
    if let Operand::Constant(c) = left {
        if is_power_of_two(*c) {
            return Some(Instruction::Binary {
                dest,
                op: BinaryOp::ShiftLeft,
                left: right.clone(),
                right: Operand::Constant(log2(*c)),
            });
        }
    }
    None
}

fn reduce_div(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x / (power of 2) → x >> log2(power)
    if let Operand::Constant(c) = right {
        if is_power_of_two(*c) {
            return Some(Instruction::Binary {
                dest,
                op: BinaryOp::ShiftRight,
                left: left.clone(),
                right: Operand::Constant(log2(*c)),
            });
        }
    }
    None
}

fn reduce_mod(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x % (power of 2) → x & (power - 1)
    if let Operand::Constant(c) = right {
        if is_power_of_two(*c) {
            return Some(Instruction::Binary {
                dest,
                op: BinaryOp::BitwiseAnd,
                left: left.clone(),
                right: Operand::Constant(c - 1),
            });
        }
    }
    None
}
