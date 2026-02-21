use ir::{Function, Instruction, Operand};
use model::{BinaryOp, UnaryOp};

/// Algebraic simplification: apply algebraic identities to simplify expressions
///
/// Examples:
/// - x * 0 = 0, x * 1 = x
/// - x + 0 = x, x - 0 = x
/// - x & 0 = 0, x | 0 = x
/// - x ^ 0 = x, x << 0 = x, x >> 0 = x
pub fn algebraic_simplification(func: &mut Function) {
    // First pass: build a map from VarId to its defining instruction
    // so we can detect double-negation patterns like ~~x, -(-x),
    // and chain add/sub constants like (x+a)+b → x+(a+b).
    let mut var_def: std::collections::HashMap<ir::VarId, Instruction> = std::collections::HashMap::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Unary { dest, .. } | Instruction::Copy { dest, .. }
                | Instruction::Binary { dest, .. } => {
                    var_def.insert(*dest, inst.clone());
                }
                _ => {}
            }
        }
    }

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
                    if let Some(simplified) = try_simplify_binary(&op, &left, &right, dest, &var_def) {
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
                Instruction::Unary { dest, op, src } => {
                    if let Some(simplified) = try_simplify_unary(&op, &src, dest, &var_def) {
                        new_instructions.push(simplified);
                    } else {
                        new_instructions.push(Instruction::Unary { dest, op, src });
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
    var_def: &std::collections::HashMap<ir::VarId, Instruction>,
) -> Option<Instruction> {
    // First try basic simplifications
    let basic = match op {
        BinaryOp::Mul => simplify_mul(left, right, dest),
        BinaryOp::Div => simplify_div(left, right, dest),
        BinaryOp::Mod => simplify_mod(left, right, dest),
        BinaryOp::Add => simplify_add(left, right, dest),
        BinaryOp::Sub => simplify_sub(left, right, dest),
        BinaryOp::BitwiseAnd => simplify_and(left, right, dest),
        BinaryOp::BitwiseOr => simplify_or(left, right, dest),
        BinaryOp::BitwiseXor => simplify_xor(left, right, dest),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => simplify_shift(op, left, right, dest),
        BinaryOp::EqualEqual | BinaryOp::NotEqual
        | BinaryOp::Less | BinaryOp::LessEqual
        | BinaryOp::Greater | BinaryOp::GreaterEqual => simplify_comparison(op, left, right, dest),
        _ => None,
    };
    if basic.is_some() {
        return basic;
    }
    
    // Chain add/sub constants: (x + a) + b → x + (a + b)
    if let Operand::Constant(c2) = right {
        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            if let Operand::Var(src_var) = left {
                if let Some(Instruction::Binary {
                    op: prev_op,
                    left: orig_left,
                    right: Operand::Constant(c1),
                    ..
                }) = var_def.get(src_var) {
                    if matches!(prev_op, BinaryOp::Add | BinaryOp::Sub) {
                        // Combine: (x +/- a) +/- b
                        let effective_c1 = if *prev_op == BinaryOp::Sub { -c1 } else { *c1 };
                        let effective_c2 = if *op == BinaryOp::Sub { -c2 } else { *c2 };
                        let combined = effective_c1 + effective_c2;
                        
                        if combined >= 0 {
                            return Some(Instruction::Binary {
                                dest,
                                op: BinaryOp::Add,
                                left: orig_left.clone(),
                                right: Operand::Constant(combined),
                            });
                        } else {
                            return Some(Instruction::Binary {
                                dest,
                                op: BinaryOp::Sub,
                                left: orig_left.clone(),
                                right: Operand::Constant(-combined),
                            });
                        }
                    }
                }
            }
        }
    }
    
    None
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
    // x * -1 = -x
    if matches!(right, Operand::Constant(-1)) {
        return Some(Instruction::Unary {
            dest,
            op: UnaryOp::Minus,
            src: left.clone(),
        });
    }
    // -1 * x = -x
    if matches!(left, Operand::Constant(-1)) {
        return Some(Instruction::Unary {
            dest,
            op: UnaryOp::Minus,
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
    // x / -1 = -x
    if matches!(right, Operand::Constant(-1)) {
        return Some(Instruction::Unary {
            dest,
            op: UnaryOp::Minus,
            src: left.clone(),
        });
    }
    // x / x = 1 (assuming x != 0)
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            return Some(Instruction::Copy {
                dest,
                src: Operand::Constant(1),
            });
        }
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
    // For same-variable comparisons, apply identity rules:
    //   x == x → 1,  x != x → 0
    //   x <  x → 0,  x <= x → 1
    //   x >  x → 0,  x >= x → 1
    if let (Operand::Var(v1), Operand::Var(v2)) = (left, right) {
        if v1 == v2 {
            let result = match op {
                BinaryOp::EqualEqual | BinaryOp::LessEqual | BinaryOp::GreaterEqual => 1,
                BinaryOp::NotEqual | BinaryOp::Less | BinaryOp::Greater => 0,
                _ => return None,
            };
            return Some(Instruction::Copy {
                dest,
                src: Operand::Constant(result),
            });
        }
    }
    
    // Constant comparison folding: both sides are constants
    if let (Operand::Constant(a), Operand::Constant(b)) = (left, right) {
        let result = match op {
            BinaryOp::EqualEqual => if a == b { 1 } else { 0 },
            BinaryOp::NotEqual => if a != b { 1 } else { 0 },
            BinaryOp::Less => if a < b { 1 } else { 0 },
            BinaryOp::LessEqual => if a <= b { 1 } else { 0 },
            BinaryOp::Greater => if a > b { 1 } else { 0 },
            BinaryOp::GreaterEqual => if a >= b { 1 } else { 0 },
            _ => return None,
        };
        return Some(Instruction::Copy {
            dest,
            src: Operand::Constant(result),
        });
    }
    
    // Normalize: constant on right side for consistent pattern matching
    // If left is constant and right is var, flip the comparison
    if let (Operand::Constant(_), Operand::Var(_)) = (left, right) {
        let flipped_op = match op {
            BinaryOp::Less => BinaryOp::Greater,
            BinaryOp::LessEqual => BinaryOp::GreaterEqual,
            BinaryOp::Greater => BinaryOp::Less,
            BinaryOp::GreaterEqual => BinaryOp::LessEqual,
            _ => op.clone(),  // == and != are symmetric
        };
        return Some(Instruction::Binary {
            dest,
            op: flipped_op,
            left: right.clone(),
            right: left.clone(),
        });
    }
    
    None
}

/// Simplify unary operations using algebraic identities:
/// - `~~x` → `x`           (double bitwise NOT)
/// - `-(-x)` → `x`         (double arithmetic negation)
/// These patterns appear in kernel macros and get generated by macro expansion.
fn try_simplify_unary(
    op: &UnaryOp,
    src: &Operand,
    dest: ir::VarId,
    var_def: &std::collections::HashMap<ir::VarId, Instruction>,
) -> Option<Instruction> {
    // Check if `src` is itself defined by the same unary op → double application
    if let Operand::Var(src_var) = src {
        if let Some(Instruction::Unary { op: inner_op, src: inner_src, .. }) = var_def.get(src_var) {
            match (op, inner_op) {
                // ~~x → x
                (UnaryOp::BitwiseNot, UnaryOp::BitwiseNot) => {
                    return Some(Instruction::Copy {
                        dest,
                        src: inner_src.clone(),
                    });
                }
                // -(-x) → x
                (UnaryOp::Minus, UnaryOp::Minus) => {
                    return Some(Instruction::Copy {
                        dest,
                        src: inner_src.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    None
}
