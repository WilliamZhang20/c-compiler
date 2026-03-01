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
    inst.for_each_use(|v| { used_vars.insert(v); });
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
    if inst.has_side_effects() {
        return true;
    }
    // Pure computations - only keep if result is used
    match inst.dest() {
        Some(dest) => used_vars.contains(&dest),
        None => true, // No dest and no side-effects shouldn't happen, keep to be safe
    }
}
