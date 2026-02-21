use ir::{Function, Instruction, Operand, VarId};
use model::BinaryOp;
use crate::utils::{is_power_of_two, log2};
use std::collections::HashMap;

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

    // Second pass: combine consecutive shifts
    combine_consecutive_shifts(func);
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
        // x * (2^n - 1)  →  (x << n) - x  (e.g., x*3, x*7, x*15)
        if *c > 2 && is_power_of_two(*c + 1) {
            return None; // Could decompose but needs temp var; skip for now
        }
        // x * (2^n + 1)  →  (x << n) + x  (e.g., x*3=x*2+x, x*5=x*4+x, x*9=x*8+x)
        if *c > 2 && is_power_of_two(*c - 1) {
            return None; // Would need temp var; covered by strength reduction + algebraic combo
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

/// Combine consecutive shifts in the same direction.
/// If `t1 = x << a` and `t2 = t1 << b`, replace the second with `t2 = x << (a+b)`.
/// Same for `>>`. This is useful after strength reduction converts multiplies to shifts.
fn combine_consecutive_shifts(func: &mut Function) {
    // Build a map of VarId → (shift_op, source_operand, shift_amount)
    // for all shift-by-constant instructions
    let mut shift_defs: HashMap<VarId, (BinaryOp, Operand, i64)> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Binary { dest, op, left, right: Operand::Constant(amt) } = inst {
                if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
                    shift_defs.insert(*dest, (op.clone(), left.clone(), *amt));
                }
            }
        }
    }

    // Now look for shifts whose source was also a shift in the same direction
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            if let Instruction::Binary { dest, op, left: Operand::Var(src_var), right: Operand::Constant(amt) } = inst {
                if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
                    if let Some((prev_op, orig_src, prev_amt)) = shift_defs.get(src_var) {
                        if prev_op == op {
                            // Combine: (x << a) << b → x << (a + b)
                            let combined = prev_amt + *amt;
                            if combined < 64 {
                                let d = *dest;
                                let o = op.clone();
                                *inst = Instruction::Binary {
                                    dest: d,
                                    op: o,
                                    left: orig_src.clone(),
                                    right: Operand::Constant(combined),
                                };
                            }
                        }
                    }
                }
            }
        }
    }
}
