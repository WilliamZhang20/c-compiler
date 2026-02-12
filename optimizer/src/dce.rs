use ir::{Function, Instruction, Operand, VarId};
use std::collections::HashSet;

/// Dead code elimination: remove instructions that compute unused values
///
/// Identifies variables that are never used and removes instructions that
/// define them (as long as those instructions have no side effects).
pub fn dce_function(func: &mut Function) -> bool {
    let mut changed = false;
    let used_vars = collect_used_vars(func);

    // Remove instructions that define unused variables (without side effects)
    for block in &mut func.blocks {
        let initial_count = block.instructions.len();
        block.instructions.retain(|inst| should_retain(inst, &used_vars));

        if block.instructions.len() < initial_count {
            changed = true;
        }
    }

    changed
}

fn collect_used_vars(func: &Function) -> HashSet<VarId> {
    let mut used_vars = HashSet::new();

    for block in &func.blocks {
        for inst in &block.instructions {
            collect_uses_from_instruction(inst, &mut used_vars);
        }

        collect_uses_from_terminator(&block.terminator, &mut used_vars);
    }

    used_vars
}

fn collect_uses_from_instruction(inst: &Instruction, used_vars: &mut HashSet<VarId>) {
    match inst {
        Instruction::Binary { left, right, .. } => {
            add_operand_var(left, used_vars);
            add_operand_var(right, used_vars);
        }
        Instruction::FloatBinary { left, right, .. } => {
            add_operand_var(left, used_vars);
            add_operand_var(right, used_vars);
        }
        Instruction::Unary { src, .. } => {
            add_operand_var(src, used_vars);
        }
        Instruction::FloatUnary { src, .. } => {
            add_operand_var(src, used_vars);
        }
        Instruction::Copy { src, .. } => {
            add_operand_var(src, used_vars);
        }
        Instruction::Cast { src, .. } => {
            add_operand_var(src, used_vars);
        }
        Instruction::Call { args, .. } => {
            for arg in args {
                add_operand_var(arg, used_vars);
            }
        }
        Instruction::IndirectCall { func_ptr, args, .. } => {
            add_operand_var(func_ptr, used_vars);
            for arg in args {
                add_operand_var(arg, used_vars);
            }
        }
        Instruction::Load { addr, .. } => {
            add_operand_var(addr, used_vars);
        }
        Instruction::Store { addr, src, .. } => {
            add_operand_var(addr, used_vars);
            add_operand_var(src, used_vars);
        }
        Instruction::GetElementPtr { base, index, .. } => {
            add_operand_var(base, used_vars);
            add_operand_var(index, used_vars);
        }
        Instruction::Phi { preds, .. } => {
            for (_, v) in preds {
                used_vars.insert(*v);
            }
        }
        Instruction::InlineAsm { inputs, .. } => {
            for input in inputs {
                add_operand_var(input, used_vars);
            }
        }
        Instruction::Alloca { .. } => {}
    }
}

fn collect_uses_from_terminator(terminator: &ir::Terminator, used_vars: &mut HashSet<VarId>) {
    match terminator {
        ir::Terminator::CondBr { cond, .. } => {
            add_operand_var(cond, used_vars);
        }
        ir::Terminator::Ret(Some(op)) => {
            add_operand_var(op, used_vars);
        }
        _ => {}
    }
}

fn add_operand_var(op: &Operand, used_vars: &mut HashSet<VarId>) {
    if let Operand::Var(v) = op {
        used_vars.insert(*v);
    }
}

fn should_retain(inst: &Instruction, used_vars: &HashSet<VarId>) -> bool {
    match inst {
        // Pure computations - only keep if result is used
        Instruction::Binary { dest, .. }
        | Instruction::FloatBinary { dest, .. }
        | Instruction::Unary { dest, .. }
        | Instruction::FloatUnary { dest, .. }
        | Instruction::Copy { dest, .. }
        | Instruction::Cast { dest, .. }
        | Instruction::Load { dest, .. }
        | Instruction::GetElementPtr { dest, .. }
        | Instruction::Phi { dest, .. } => used_vars.contains(dest),

        // Side effects or essential instructions - always keep
        Instruction::Call { .. }
        | Instruction::IndirectCall { .. }
        | Instruction::Store { .. }
        | Instruction::InlineAsm { .. }
        | Instruction::Alloca { .. } => true,
    }
}
