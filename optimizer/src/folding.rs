use ir::{Function, Instruction, Operand, VarId};
use model::{BinaryOp, UnaryOp, Type};
use std::collections::HashMap;

/// Constant folding and propagation
///
/// Performs repeated passes of constant folding until no more changes occur.
/// Evaluates expressions with constant operands at compile time and propagates
/// the results through the function.
pub fn optimize_function(func: &mut Function) {
    let mut changed = true;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10;

    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;

        let mut constants: HashMap<VarId, i64> = HashMap::new();
        let mut float_constants: HashMap<VarId, f64> = HashMap::new();

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
                        let l = resolve_operand(&left, &constants);
                        let r = resolve_operand(&right, &constants);

                        if let (Operand::Constant(lc), Operand::Constant(rc)) = (&l, &r) {
                            if let Some(val) = fold_binary(op.clone(), *lc, *rc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(val),
                                });
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::Binary {
                            dest,
                            op,
                            left: l,
                            right: r,
                        });
                    }
                    Instruction::FloatBinary {
                        dest,
                        op,
                        left,
                        right,
                    } => {
                        let l = resolve_float_operand(&left, &constants, &float_constants);
                        let r = resolve_float_operand(&right, &constants, &float_constants);

                        if let (Operand::FloatConstant(lf), Operand::FloatConstant(rf)) = (&l, &r) {
                            if let Some(val) = fold_float_binary(&op, *lf, *rf) {
                                match val {
                                    FloatFoldResult::Float(f) => {
                                        float_constants.insert(dest, f);
                                        new_instructions.push(Instruction::Copy {
                                            dest,
                                            src: Operand::FloatConstant(f),
                                        });
                                    }
                                    FloatFoldResult::Int(i) => {
                                        constants.insert(dest, i);
                                        new_instructions.push(Instruction::Copy {
                                            dest,
                                            src: Operand::Constant(i),
                                        });
                                    }
                                }
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::FloatBinary {
                            dest,
                            op,
                            left: l,
                            right: r,
                        });
                    }
                    Instruction::Unary { dest, op, src } => {
                        let s = resolve_operand(&src, &constants);

                        if let Operand::Constant(sc) = s {
                            if let Some(val) = fold_unary(op.clone(), sc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(val),
                                });
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::Unary { dest, op, src: s });
                    }
                    Instruction::FloatUnary { dest, op, src } => {
                        let s = resolve_float_operand(&src, &constants, &float_constants);

                        if let Operand::FloatConstant(sf) = s {
                            if let Some(val) = fold_float_unary(&op, sf) {
                                match val {
                                    FloatFoldResult::Float(f) => {
                                        float_constants.insert(dest, f);
                                        new_instructions.push(Instruction::Copy {
                                            dest,
                                            src: Operand::FloatConstant(f),
                                        });
                                    }
                                    FloatFoldResult::Int(i) => {
                                        constants.insert(dest, i);
                                        new_instructions.push(Instruction::Copy {
                                            dest,
                                            src: Operand::Constant(i),
                                        });
                                    }
                                }
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::FloatUnary { dest, op, src: s });
                    }
                    Instruction::Copy { dest, src } => {
                        let s = resolve_operand(&src, &constants);
                        if let Operand::Constant(sc) = &s {
                            constants.insert(dest, *sc);
                        } else if let Operand::FloatConstant(fc) = &s {
                            float_constants.insert(dest, *fc);
                        }
                        // Also check if a float var is being copied
                        let s = resolve_float_operand(&s, &constants, &float_constants);
                        if let Operand::FloatConstant(fc) = &s {
                            float_constants.insert(dest, *fc);
                        }
                        new_instructions.push(Instruction::Copy { dest, src: s });
                    }
                    Instruction::Cast { dest, src, r#type } => {
                        let s = resolve_float_operand(&src, &constants, &float_constants);
                        // Int constant → float type
                        if let Operand::Constant(val) = &s {
                            if r#type == Type::Float || r#type == Type::Double {
                                let f = *val as f64;
                                float_constants.insert(dest, f);
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::FloatConstant(f),
                                });
                                changed = true;
                                continue;
                            }
                            if let Some(folded) = fold_cast(*val, &r#type) {
                                constants.insert(dest, folded);
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(folded),
                                });
                                changed = true;
                                continue;
                            }
                        }
                        // Float constant → int type
                        if let Operand::FloatConstant(f) = &s {
                            match &r#type {
                                Type::Float | Type::Double => {
                                    // float → float cast (e.g., double → float)
                                    let f_val = if r#type == Type::Float { (*f as f32) as f64 } else { *f };
                                    float_constants.insert(dest, f_val);
                                    new_instructions.push(Instruction::Copy {
                                        dest,
                                        src: Operand::FloatConstant(f_val),
                                    });
                                    changed = true;
                                    continue;
                                }
                                _ => {
                                    // float → int
                                    let i = *f as i64;
                                    if let Some(folded) = fold_cast(i, &r#type) {
                                        constants.insert(dest, folded);
                                        new_instructions.push(Instruction::Copy {
                                            dest,
                                            src: Operand::Constant(folded),
                                        });
                                        changed = true;
                                        continue;
                                    }
                                }
                            }
                        }
                        new_instructions.push(Instruction::Cast { dest, src: s, r#type });
                    }
                    Instruction::Call { dest, name, args } => {
                        let resolved_args: Vec<_> =
                            args.iter().map(|arg| resolve_operand(arg, &constants)).collect();
                        new_instructions.push(Instruction::Call {
                            dest,
                            name,
                            args: resolved_args,
                        });
                    }
                    Instruction::IndirectCall {
                        dest,
                        func_ptr,
                        args,
                    } => {
                        let resolved_func_ptr = resolve_operand(&func_ptr, &constants);
                        let resolved_args: Vec<_> =
                            args.iter().map(|arg| resolve_operand(arg, &constants)).collect();
                        new_instructions.push(Instruction::IndirectCall {
                            dest,
                            func_ptr: resolved_func_ptr,
                            args: resolved_args,
                        });
                    }
                    Instruction::Load { dest, addr, value_type } => {
                        new_instructions.push(Instruction::Load {
                            dest,
                            addr: resolve_operand(&addr, &constants),
                            value_type,
                        });
                    }
                    Instruction::Store { addr, src, value_type } => {
                        new_instructions.push(Instruction::Store {
                            addr: resolve_operand(&addr, &constants),
                            src: resolve_operand(&src, &constants),
                            value_type,
                        });
                    }
                    Instruction::GetElementPtr {
                        dest,
                        base,
                        index,
                        element_type,
                    } => {
                        new_instructions.push(Instruction::GetElementPtr {
                            dest,
                            base: resolve_operand(&base, &constants),
                            index: resolve_operand(&index, &constants),
                            element_type,
                        });
                    }
                    other => new_instructions.push(other),
                }
            }
            block.instructions = new_instructions;

            // Fold terminator conditions
            match &mut block.terminator {
                ir::Terminator::CondBr {
                    cond,
                    then_block,
                    else_block,
                } => {
                    let c = resolve_operand(cond, &constants);
                    if let Operand::Constant(val) = c {
                        let target = if val != 0 { *then_block } else { *else_block };
                        block.terminator = ir::Terminator::Br(target);
                        changed = true;
                    } else {
                        *cond = c;
                    }
                }
                ir::Terminator::Ret(Some(op)) => {
                    *op = resolve_operand(op, &constants);
                }
                _ => {}
            }
        }

        // Run DCE after each folding pass
        changed |= crate::dce::dce_function(func);
    }

    if iterations >= MAX_ITERATIONS {
        // eprintln!(
        //     "Warning: Optimizer reached max iterations ({}) for function {}",
        //     MAX_ITERATIONS, func.name
        // );
    }
}

fn resolve_operand(op: &Operand, constants: &HashMap<VarId, i64>) -> Operand {
    match op {
        Operand::Var(v) => constants
            .get(v)
            .map(|&c| Operand::Constant(c))
            .unwrap_or_else(|| op.clone()),
        _ => op.clone(),
    }
}

/// Resolve an operand, checking both int and float constant maps.
/// If it's a Var known to be a float constant, return FloatConstant.
/// Also resolves int constants via the regular map.
fn resolve_float_operand(op: &Operand, constants: &HashMap<VarId, i64>, float_constants: &HashMap<VarId, f64>) -> Operand {
    match op {
        Operand::Var(v) => {
            if let Some(&f) = float_constants.get(v) {
                Operand::FloatConstant(f)
            } else if let Some(&c) = constants.get(v) {
                Operand::Constant(c)
            } else {
                op.clone()
            }
        }
        _ => op.clone(),
    }
}

enum FloatFoldResult {
    Float(f64),
    Int(i64),
}

fn fold_float_binary(op: &BinaryOp, l: f64, r: f64) -> Option<FloatFoldResult> {
    match op {
        BinaryOp::Add => Some(FloatFoldResult::Float(l + r)),
        BinaryOp::Sub => Some(FloatFoldResult::Float(l - r)),
        BinaryOp::Mul => Some(FloatFoldResult::Float(l * r)),
        BinaryOp::Div => Some(FloatFoldResult::Float(l / r)), // IEEE 754: div by 0 → ±Inf, NaN propagates
        // Comparisons return integers (0 or 1), with IEEE 754 NaN semantics
        BinaryOp::EqualEqual => Some(FloatFoldResult::Int((l == r) as i64)),
        BinaryOp::NotEqual => Some(FloatFoldResult::Int((l != r) as i64)),
        BinaryOp::Less => Some(FloatFoldResult::Int((l < r) as i64)),
        BinaryOp::LessEqual => Some(FloatFoldResult::Int((l <= r) as i64)),
        BinaryOp::Greater => Some(FloatFoldResult::Int((l > r) as i64)),
        BinaryOp::GreaterEqual => Some(FloatFoldResult::Int((l >= r) as i64)),
        _ => None,
    }
}

fn fold_float_unary(op: &UnaryOp, s: f64) -> Option<FloatFoldResult> {
    match op {
        UnaryOp::Minus => Some(FloatFoldResult::Float(-s)),
        UnaryOp::Plus => Some(FloatFoldResult::Float(s)),
        UnaryOp::LogicalNot => Some(FloatFoldResult::Int((s == 0.0) as i64)),
        _ => None,
    }
}

pub fn fold_binary(op: BinaryOp, l: i64, r: i64) -> Option<i64> {
    match op {
        BinaryOp::Add => Some(l + r),
        BinaryOp::Sub => Some(l - r),
        BinaryOp::Mul => Some(l * r),
        BinaryOp::Div => {
            if r != 0 {
                Some(l / r)
            } else {
                None
            }
        }
        BinaryOp::Mod => {
            if r != 0 {
                Some(l % r)
            } else {
                None
            }
        }
        BinaryOp::EqualEqual => Some((l == r) as i64),
        BinaryOp::NotEqual => Some((l != r) as i64),
        BinaryOp::Less => Some((l < r) as i64),
        BinaryOp::LessEqual => Some((l <= r) as i64),
        BinaryOp::Greater => Some((l > r) as i64),
        BinaryOp::GreaterEqual => Some((l >= r) as i64),
        BinaryOp::BitwiseAnd => Some(l & r),
        BinaryOp::BitwiseOr => Some(l | r),
        BinaryOp::BitwiseXor => Some(l ^ r),
        BinaryOp::ShiftLeft => if r >= 0 && r < 64 { Some(l << r) } else { None },
        BinaryOp::ShiftRight => if r >= 0 && r < 64 { Some(l >> r) } else { None },
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::Assign => None,
        _ => None,
    }
}

pub fn fold_unary(op: UnaryOp, s: i64) -> Option<i64> {
    match op {
        UnaryOp::Minus => Some(-s),
        UnaryOp::Plus => Some(s),
        UnaryOp::LogicalNot => Some((s == 0) as i64),
        UnaryOp::BitwiseNot => Some(!s),
        UnaryOp::AddrOf | UnaryOp::Deref => None,
    }
}

/// Fold a cast of a constant integer to a target type at compile time.
/// Simulates the truncation/extension that would happen at runtime.
fn fold_cast(val: i64, target: &Type) -> Option<i64> {
    match target {
        Type::Char          => Some(val as i8 as i64),
        Type::UnsignedChar  => Some(val as u8 as i64),
        Type::Short         => Some(val as i16 as i64),
        Type::UnsignedShort => Some(val as u16 as i64),
        Type::Int           => Some(val as i32 as i64),
        Type::UnsignedInt   => Some(val as u32 as i64),
        Type::Long | Type::LongLong => Some(val),          // same width on x86-64
        Type::UnsignedLong | Type::UnsignedLongLong => Some(val), // bit pattern preserved
        _ => None, // Pointer, float, struct, etc. — don't fold
    }
}
