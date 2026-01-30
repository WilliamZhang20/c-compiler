use ir::{IRProgram, Function, Instruction, Operand, VarId};
use model::{BinaryOp, UnaryOp};
use std::collections::HashMap;

pub fn optimize(mut program: IRProgram) -> IRProgram {
    for func in &mut program.functions {
        optimize_function(func);
    }
    program
}

fn optimize_function(func: &mut Function) {
    let mut changed = true;
    while changed {
        changed = false;
        let mut constants: HashMap<VarId, i64> = HashMap::new();

        for block in &mut func.blocks {
            let mut new_instructions = Vec::new();
            for inst in block.instructions.drain(..) {
                match inst {
                    Instruction::Binary { dest, op, left, right } => {
                        let l = resolve_operand(&left, &constants);
                        let r = resolve_operand(&right, &constants);
                        if let (Operand::Constant(lc), Operand::Constant(rc)) = (&l, &r) {
                            if let Some(val) = fold_binary(op.clone(), *lc, *rc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy { dest, src: Operand::Constant(val) });
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::Binary { dest, op, left: l, right: r });
                    }
                    Instruction::Unary { dest, op, src } => {
                        let s = resolve_operand(&src, &constants);
                        if let Operand::Constant(sc) = s {
                            if let Some(val) = fold_unary(op.clone(), sc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy { dest, src: Operand::Constant(val) });
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
                        let mut resolved_args = Vec::new();
                        for arg in args {
                            resolved_args.push(resolve_operand(&arg, &constants));
                        }
                        new_instructions.push(Instruction::Call { dest, name, args: resolved_args });
                    }
                    _ => new_instructions.push(inst),
                }
            }
            block.instructions = new_instructions;

            // Also fold terminator
            match &mut block.terminator {
                ir::Terminator::CondBr { cond, then_block, else_block } => {
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
    }
}

fn resolve_operand(op: &Operand, constants: &HashMap<VarId, i64>) -> Operand {
    match op {
        Operand::Var(v) => {
            if let Some(c) = constants.get(v) {
                Operand::Constant(*c)
            } else {
                op.clone()
            }
        }
        _ => op.clone(),
    }
}

fn fold_binary(op: BinaryOp, l: i64, r: i64) -> Option<i64> {
    match op {
        BinaryOp::Add => Some(l + r),
        BinaryOp::Sub => Some(l - r),
        BinaryOp::Mul => Some(l * r),
        BinaryOp::Div => if r != 0 { Some(l / r) } else { None },
        BinaryOp::EqualEqual => Some((l == r) as i64),
        BinaryOp::NotEqual => Some((l != r) as i64),
        BinaryOp::Less => Some((l < r) as i64),
        BinaryOp::LessEqual => Some((l <= r) as i64),
        BinaryOp::Greater => Some((l > r) as i64),
        BinaryOp::GreaterEqual => Some((l >= r) as i64),
        _ => None,
    }
}

fn fold_unary(op: UnaryOp, s: i64) -> Option<i64> {
    match op {
        UnaryOp::Minus => Some(-s),
        UnaryOp::Plus => Some(s),
        UnaryOp::LogicalNot => Some((s == 0) as i64),
        UnaryOp::AddrOf | UnaryOp::Deref => None,
    }
}
