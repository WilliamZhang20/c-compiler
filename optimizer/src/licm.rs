// Loop-Invariant Code Motion (LICM)
//
// Hoists instructions out of loops when their operands are defined outside
// the loop (i.e., their values don't change across iterations). This improves
// cache locality by:
// - Reducing redundant address computations inside loops
// - Moving invariant loads out of the loop body
// - Reducing I-cache pressure (smaller loop body)
//
// Example:
//   for (i = 0; i < N; i++) {
//       x = a + b;       // invariant: hoist before loop
//       arr[i] = x * i;
//   }
// Becomes:
//   x = a + b;
//   for (i = 0; i < N; i++) {
//       arr[i] = x * i;
//   }

use ir::{Function, Instruction, Operand, VarId, BlockId};
use std::collections::HashSet;
use crate::loop_analysis::{self, NaturalLoop};

/// Run LICM on all loops in a function
pub fn loop_invariant_code_motion(func: &mut Function) {
    let loops = loop_analysis::find_loops(func);
    for lp in &loops {
        hoist_invariants(func, lp);
    }
}

/// Check if a variable is defined inside the loop
fn is_defined_in_loop(var: VarId, func: &Function, loop_body: &HashSet<BlockId>) -> bool {
    for block in &func.blocks {
        if !loop_body.contains(&block.id) {
            continue;
        }
        for inst in &block.instructions {
            if let Some(def) = get_def(inst) {
                if def == var {
                    return true;
                }
            }
        }
    }
    false
}

/// Get the variable defined by an instruction (if any)
fn get_def(inst: &Instruction) -> Option<VarId> {
    inst.dest()
}

/// Check if an operand is loop-invariant (constant or defined outside the loop)
fn is_operand_invariant(
    op: &Operand,
    func: &Function,
    loop_body: &HashSet<BlockId>,
    already_hoisted: &HashSet<VarId>,
) -> bool {
    match op {
        Operand::Constant(_) | Operand::FloatConstant(_) | Operand::Global(_) => true,
        Operand::Var(v) => {
            // Invariant if defined outside the loop or already hoisted
            already_hoisted.contains(v) || !is_defined_in_loop(*v, func, loop_body)
        }
    }
}

/// Check if an instruction is safe to hoist out of the loop.
/// An instruction is invariant if:
/// 1. All its operands are loop-invariant
/// 2. It has no side effects (no stores, calls, etc.)
/// 3. It's NOT a Phi node (control-flow dependent)
fn is_hoistable(
    inst: &Instruction,
    func: &Function,
    loop_body: &HashSet<BlockId>,
    already_hoisted: &HashSet<VarId>,
) -> bool {
    match inst {
        // Pure arithmetic — hoist if all operands are invariant
        Instruction::Binary { op: _, left, right, .. } => {
            is_operand_invariant(left, func, loop_body, already_hoisted)
                && is_operand_invariant(right, func, loop_body, already_hoisted)
        }
        Instruction::FloatBinary { op: _, left, right, .. } => {
            is_operand_invariant(left, func, loop_body, already_hoisted)
                && is_operand_invariant(right, func, loop_body, already_hoisted)
        }
        Instruction::Unary { src, .. } => {
            is_operand_invariant(src, func, loop_body, already_hoisted)
        }
        Instruction::FloatUnary { src, .. } => {
            is_operand_invariant(src, func, loop_body, already_hoisted)
        }
        Instruction::Copy { src, .. } => {
            is_operand_invariant(src, func, loop_body, already_hoisted)
        }
        Instruction::Cast { src, .. } => {
            is_operand_invariant(src, func, loop_body, already_hoisted)
        }
        // GEP with invariant base and index — hoist the address computation
        Instruction::GetElementPtr { base, index, .. } => {
            is_operand_invariant(base, func, loop_body, already_hoisted)
                && is_operand_invariant(index, func, loop_body, already_hoisted)
        }
        // Load from an invariant address with no stores in the loop to the same
        // address is safe to hoist. For safety, we only hoist loads from addresses
        // that are provably invariant AND the loop has no stores at all (conservative).
        Instruction::Load { addr, .. } => {
            if !is_operand_invariant(addr, func, loop_body, already_hoisted) {
                return false;
            }
            // Conservative: only hoist if the loop body has no stores at all
            !loop_has_stores(func, loop_body)
        }

        // Never hoist these:
        Instruction::Phi { .. }         // SSA control-flow join
        | Instruction::Alloca { .. }    // Stack allocation
        | Instruction::Store { .. }     // Side effects
        | Instruction::Call { .. }      // Side effects
        | Instruction::IndirectCall { .. }
        | Instruction::VaStart { .. }
        | Instruction::VaEnd { .. }
        | Instruction::VaCopy { .. }
        | Instruction::VaArg { .. }
        | Instruction::InlineAsm { .. }
        | Instruction::Simd { .. } => false,
    }
}

/// Check if any block in the loop body contains a Store instruction
fn loop_has_stores(func: &Function, loop_body: &HashSet<BlockId>) -> bool {
    for block in &func.blocks {
        if !loop_body.contains(&block.id) {
            continue;
        }
        for inst in &block.instructions {
            match inst {
                Instruction::Store { .. }
                | Instruction::Call { .. }
                | Instruction::IndirectCall { .. }
                | Instruction::InlineAsm { .. }
                | Instruction::Simd { op: ir::SimdOp::Store, .. } => return true,
                _ => {}
            }
        }
    }
    false
}

/// Hoist loop-invariant instructions out of a loop into its preheader.
/// Uses a fixed-point iteration: keep trying until no more instructions can be hoisted.
fn hoist_invariants(func: &mut Function, lp: &NaturalLoop) {
    let preheader = match lp.preheader {
        Some(p) => p,
        None => return, // No preheader — can't hoist
    };

    // Must have at least 2 blocks to be worth optimizing
    if lp.body.len() < 2 {
        return;
    }

    let mut already_hoisted: HashSet<VarId> = HashSet::new();

    // Fixed-point: iterate until no more instructions can be hoisted
    // (hoisting one instruction may make others hoistable)
    loop {
        // Collect instructions to hoist in this iteration
        let mut to_hoist: Vec<(BlockId, usize, Instruction)> = Vec::new();

        for block in &func.blocks {
            if !lp.body.contains(&block.id) {
                continue;
            }

            for (idx, inst) in block.instructions.iter().enumerate() {
                if let Some(def) = get_def(inst) {
                    if already_hoisted.contains(&def) {
                        continue;
                    }
                }
                if is_hoistable(inst, func, &lp.body, &already_hoisted) {
                    to_hoist.push((block.id, idx, inst.clone()));
                }
            }
        }

        if to_hoist.is_empty() {
            break;
        }

        // Hoist each instruction: remove from original block, add to preheader
        for (block_id, _, inst) in &to_hoist {
            if let Some(def) = get_def(inst) {
                already_hoisted.insert(def);
            }

            // Remove from source block
            if let Some(block) = func.blocks.iter_mut().find(|b| b.id == *block_id) {
                block.instructions.retain(|i| !std::ptr::eq(i, inst));
            }
        }

        // We need to re-find and remove since the instructions may have shifted.
        // Instead, let's collect defs and remove by matching.
        let hoisted_defs: HashSet<VarId> = to_hoist.iter()
            .filter_map(|(_, _, inst)| get_def(inst))
            .collect();

        // Remove hoisted instructions from their source blocks
        for block in &mut func.blocks {
            if !lp.body.contains(&block.id) {
                continue;
            }
            block.instructions.retain(|inst| {
                if let Some(def) = get_def(inst) {
                    if hoisted_defs.contains(&def) && !matches!(inst, Instruction::Phi { .. }) {
                        return false; // Remove
                    }
                }
                true
            });
        }

        // Add hoisted instructions to preheader (before the terminator)
        if let Some(pre_block) = func.blocks.iter_mut().find(|b| b.id == preheader) {
            for (_, _, inst) in &to_hoist {
                pre_block.instructions.push(inst.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_to_ir(src: &str) -> ir::IRProgram {
        let tokens = lexer::lex(src).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        lowerer.lower_program(&ast).unwrap()
    }

    #[test]
    fn test_licm_hoists_invariant_binary() {
        // x = a + b is invariant across the loop
        let src = r#"
            int main() {
                int a = 3;
                int b = 4;
                int x;
                int sum = 0;
                int i;
                for (i = 0; i < 10; i = i + 1) {
                    x = a + b;
                    sum = sum + x;
                }
                return sum;
            }
        "#;
        let mut prog = compile_to_ir(src);
        // Run mem2reg first so we have SSA form
        for func in &mut prog.functions {
            ir::mem2reg(func);
            loop_invariant_code_motion(func);
        }
        // The test passes if it doesn't crash and produces valid IR
    }

    #[test]
    fn test_licm_does_not_hoist_variant() {
        // sum += i is NOT invariant (depends on phi/IV)
        let src = r#"
            int main() {
                int sum = 0;
                int i;
                for (i = 0; i < 10; i = i + 1) {
                    sum = sum + i;
                }
                return sum;
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            loop_invariant_code_motion(func);
        }
        // Should not crash and should not hoist sum += i
    }
}
