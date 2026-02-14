use ir::{Function, Instruction, Operand};
use model::BinaryOp;

/// Algebraic simplification: apply algebraic identities to simplify expressions
///
/// Examples:
/// - x * 0 = 0, x * 1 = x
/// - x + 0 = x, x - 0 = x
/// - x & 0 = 0, x | 0 = x
/// - x ^ 0 = x, x << 0 = x, x >> 0 = x
pub fn algebraic_simplification(func: &mut Function) {
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
                    if let Some(simplified) = try_simplify_binary(&op, &left, &right, dest) {
                        new_instructions.push(simplified);
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

fn try_simplify_binary(
    op: &BinaryOp,
    left: &Operand,
    right: &Operand,
    dest: ir::VarId,
) -> Option<Instruction> {
    match op {
        BinaryOp::Mul => simplify_mul(left, right, dest),
        BinaryOp::Div => simplify_div(left, right, dest),
        BinaryOp::Mod => simplify_mod(left, right, dest),
        BinaryOp::Add => simplify_add(left, right, dest),
        BinaryOp::Sub => simplify_sub(left, right, dest),
        BinaryOp::BitwiseAnd => simplify_and(left, right, dest),
        BinaryOp::BitwiseOr => simplify_or(left, right, dest),
        BinaryOp::BitwiseXor => simplify_xor(left, right, dest),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => simplify_shift(op, left, right, dest),
        BinaryOp::EqualEqual | BinaryOp::NotEqual => simplify_comparison(op, left, right, dest),
        _ => None,
    }
}

fn simplify_mul(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x * 0 = 0 or 0 * x = 0
    if matches!(right, Operand::Constant(0)) || matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    // x * 1 = x
    if matches!(right, Operand::Constant(1)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    // 1 * x = x
    if matches!(left, Operand::Constant(1)) {
        return Some(Instruction::Copy {
            dest,
            src: right.clone(),
        });
    }
    None
}

fn simplify_div(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x / 1 = x
    if matches!(right, Operand::Constant(1)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    // 0 / x = 0 (assuming x != 0)
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    None
}

fn simplify_mod(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x % 1 = 0
    if matches!(right, Operand::Constant(1)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    // 0 % x = 0
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    None
}

fn simplify_add(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x + 0 = x
    if matches!(right, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    // 0 + x = x
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: right.clone(),
        });
    }
    None
}

fn simplify_sub(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x - 0 = x
    if matches!(right, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    // x - x = 0 (same variable)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            return Some(Instruction::Copy {
                dest,
                src: Operand::Constant(0),
            });
        }
    }
    None
}

fn simplify_and(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x & 0 = 0 or 0 & x = 0
    if matches!(right, Operand::Constant(0)) || matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    // x & -1 = x (all bits set)
    if matches!(right, Operand::Constant(-1)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    if matches!(left, Operand::Constant(-1)) {
        return Some(Instruction::Copy {
            dest,
            src: right.clone(),
        });
    }
    // x & x = x (idempotent)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            return Some(Instruction::Copy {
                dest,
                src: left.clone(),
            });
        }
    }
    None
}

fn simplify_or(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x | 0 = x
    if matches!(right, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: right.clone(),
        });
    }
    // x | -1 = -1 (all bits set)
    if matches!(right, Operand::Constant(-1)) || matches!(left, Operand::Constant(-1)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(-1),
        });
    }
    // x | x = x (idempotent)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            return Some(Instruction::Copy {
                dest,
                src: left.clone(),
            });
        }
    }
    None
}

fn simplify_xor(left: &Operand, right: &Operand, dest: ir::VarId) -> Option<Instruction> {
    // x ^ 0 = x
    if matches!(right, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: right.clone(),
        });
    }
    // x ^ x = 0 (same variable)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            return Some(Instruction::Copy {
                dest,
                src: Operand::Constant(0),
            });
        }
    }
    None
}

fn simplify_shift(
    _op: &BinaryOp,
    left: &Operand,
    right: &Operand,
    dest: ir::VarId,
) -> Option<Instruction> {
    // x << 0 = x, x >> 0 = x
    if matches!(right, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: left.clone(),
        });
    }
    // 0 << x = 0, 0 >> x = 0
    if matches!(left, Operand::Constant(0)) {
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(0),
        });
    }
    None
}

fn simplify_comparison(
    op: &BinaryOp,
    left: &Operand,
    right: &Operand,
    dest: ir::VarId,
) -> Option<Instruction> {
    // x == x is always true, x != x is always false (for same variable)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            let result = if matches!(op, BinaryOp::EqualEqual) { 1 } else { 0 };
            return Some(Instruction::Copy {
                dest,
                src: Operand::Constant(result),
            });
        }
    }
    None
}
