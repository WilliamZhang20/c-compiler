use ir::{Function, Instruction, Operand, VarId};
use model::{BinaryOp, UnaryOp};
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
                    Instruction::Copy { dest, src } => {
                        let s = resolve_operand(&src, &constants);
                        if let Operand::Constant(sc) = s {
                            constants.insert(dest, sc);
                        }
                        new_instructions.push(Instruction::Copy { dest, src: s });
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
        eprintln!(
            "Warning: Optimizer reached max iterations ({}) for function {}",
            MAX_ITERATIONS, func.name
        );
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
        BinaryOp::ShiftLeft => Some(l << r),
        BinaryOp::ShiftRight => Some(l >> r),
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
