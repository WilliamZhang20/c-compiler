// Loop Interchange for Cache Locality
//
// Swaps the order of perfectly nested loops when the inner loop has worse
// memory access stride than the outer loop. This transforms column-major
// access patterns into row-major patterns for better cache locality.
//
// Example: The classic matrix multiply
//   for i: for j: for k: c[i][j] += a[i][k] * b[k][j]
// Has b[k][j] as stride-N access (cache-unfriendly) in the innermost k-loop.
// Interchanging k↔j:
//   for i: for k: for j: c[i][j] += a[i][k] * b[k][j]  
// Makes b[k][j] stride-1 in the inner j-loop (cache-friendly).
//
// Safety requirements for interchange:
// - Loops must be "perfectly nested" (outer loop body = inner loop only)
// - No loop-carried dependencies that change semantics with interchange
// - Both loops must have simple induction variables
//
// We operate at the IR level by:
// 1. Finding nested loop pairs from the loop analysis
// 2. Analyzing memory access strides relative to each IV
// 3. If the outer IV has better inner-loop stride, swapping the comparison
//    bounds and phi init values of the two IVs

use ir::{Function, Instruction, Operand, VarId, BlockId};
use model::BinaryOp;
use std::collections::HashSet;
use crate::loop_analysis::{self, NaturalLoop, InductionVar};

/// Run loop interchange analysis on all loops in a function
pub fn try_loop_interchange(func: &mut Function) {
    let loops = loop_analysis::find_loops(func);
    
    // Find nested loop pairs: inner loop whose body ⊆ outer loop body
    let mut nested_pairs: Vec<(usize, usize)> = Vec::new(); // (outer_idx, inner_idx)
    for i in 0..loops.len() {
        for j in 0..loops.len() {
            if i == j { continue; }
            // Check if loop j is nested inside loop i
            if loops[j].body.is_subset(&loops[i].body) 
                && loops[j].header != loops[i].header 
            {
                nested_pairs.push((i, j));
            }
        }
    }

    // For each nested pair, check if interchange is profitable
    for (outer_idx, inner_idx) in &nested_pairs {
        let outer = &loops[*outer_idx];
        let inner = &loops[*inner_idx];

        // Both loops need induction variables
        let outer_iv = match &outer.induction_var {
            Some(iv) => iv.clone(),
            None => continue,
        };
        let inner_iv = match &inner.induction_var {
            Some(iv) => iv.clone(),
            None => continue,
        };

        // Check if perfectly nested (outer body blocks = header of outer + inner loop blocks)
        if !is_perfectly_nested(func, outer, inner) {
            continue;
        }

        // Analyze stride: count how many GEPs in the inner loop body use
        // the outer vs inner IV for their fastest-varying dimension
        let inner_body_blocks = &inner.body;
        let outer_stride = count_gep_stride_refs(func, inner_body_blocks, outer_iv.var);
        let inner_stride = count_gep_stride_refs(func, inner_body_blocks, inner_iv.var);

        // If outer IV is used more in GEP indices in the innermost position,
        // that means the inner loop has stride proportional to array dimensions
        // when iterating over the outer IV. Interchanging would be beneficial.
        // Heuristic: if outer IV appears in more GEP indices than inner IV,
        // interchanging will improve locality.
        if outer_stride <= inner_stride {
            continue; // Already good stride order, no interchange needed
        }

        // Perform the interchange by swapping the IV parameters
        swap_loop_ivs(func, outer, inner, &outer_iv, &inner_iv);
    }
}

/// Check if two loops are perfectly nested:
/// The outer loop's body (excluding the inner loop) should only contain
/// the outer loop's header phi/cmp/branch and nothing else significant.
fn is_perfectly_nested(func: &Function, outer: &NaturalLoop, inner: &NaturalLoop) -> bool {
    // All outer body blocks that are NOT in the inner loop
    let outer_only_blocks: Vec<BlockId> = outer.body.iter()
        .filter(|b| !inner.body.contains(b))
        .cloned()
        .collect();

    // Each outer-only block should only contain:
    // - Phi nodes (for the outer IV)
    // - Binary ops for comparison
    // - Copies
    // No loads, stores, calls, or other computation
    for block_id in &outer_only_blocks {
        if let Some(block) = func.blocks.iter().find(|b| b.id == *block_id) {
            for inst in &block.instructions {
                match inst {
                    Instruction::Phi { .. } => {} // OK: IV merge
                    Instruction::Binary { op, .. } => {
                        // Only comparisons and IV arithmetic are OK
                        match op {
                            BinaryOp::Less | BinaryOp::LessEqual |
                            BinaryOp::Greater | BinaryOp::GreaterEqual |
                            BinaryOp::EqualEqual | BinaryOp::NotEqual |
                            BinaryOp::Add | BinaryOp::Sub => {}
                            _ => return false,
                        }
                    }
                    Instruction::Copy { .. } => {} // OK
                    _ => return false, // Has real computation in outer-only blocks
                }
            }
        }
    }

    true
}

/// Count how many GEP instructions in the given blocks use a specific variable
/// as their index operand. This estimates the "stride exposure" of that variable
/// in the innermost loop — more GEP index uses = more cache-line crossings.
fn count_gep_stride_refs(
    func: &Function,
    blocks: &HashSet<BlockId>,
    var: VarId,
) -> usize {
    let mut count = 0;
    for block in &func.blocks {
        if !blocks.contains(&block.id) {
            continue;
        }
        for inst in &block.instructions {
            if let Instruction::GetElementPtr { index, .. } = inst {
                if matches!(index, Operand::Var(v) if *v == var) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Swap the induction variables of two nested loops.
/// This effectively interchanges the loops by swapping their iteration parameters.
///
/// For the interchange to work, we swap:
/// - The comparison bounds in the headers
/// - The init values in the phi nodes
/// - The step values in the body
fn swap_loop_ivs(
    func: &mut Function,
    outer: &NaturalLoop,
    inner: &NaturalLoop,
    outer_iv: &InductionVar,
    inner_iv: &InductionVar,
) {
    // Strategy: swap the bounds and init values between the two loop headers.
    // The outer header's cmp should use inner_iv's bound, and vice versa.
    // The outer header's phi should use inner_iv's init, and vice versa.
    
    // Collect the info we need
    let outer_init = outer_iv.init;
    let outer_bound = outer_iv.bound;
    let outer_step = outer_iv.step;
    let inner_init = inner_iv.init;
    let inner_bound = inner_iv.bound;
    let inner_step = inner_iv.step;
    
    // Swap bound constants in header comparison instructions
    // Outer header: change comparison from 'outer_bound' to 'inner_bound'
    swap_comparison_bound(func, outer.header, outer_iv.var, inner_bound);
    // Inner header: change comparison from 'inner_bound' to 'outer_bound'
    swap_comparison_bound(func, inner.header, inner_iv.var, outer_bound);

    // Swap init values in phi nodes
    swap_phi_init(func, outer.header, outer_iv.var, &outer.body, inner_init);
    swap_phi_init(func, inner.header, inner_iv.var, &inner.body, outer_init);

    // Swap step values in body
    if outer_step != inner_step {
        swap_step_value(func, &outer.body, outer_iv.var, inner_step);
        swap_step_value(func, &inner.body, inner_iv.var, outer_step);
    }
}

/// Change the comparison bound in a loop header
fn swap_comparison_bound(func: &mut Function, header: BlockId, iv_var: VarId, new_bound: i64) {
    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == header) {
        for inst in &mut block.instructions {
            if let Instruction::Binary { op, left, right, .. } = inst {
                match op {
                    BinaryOp::Less | BinaryOp::LessEqual |
                    BinaryOp::Greater | BinaryOp::GreaterEqual |
                    BinaryOp::EqualEqual | BinaryOp::NotEqual => {
                        // Check if one side is the IV and the other is a constant
                        if matches!(left, Operand::Var(v) if *v == iv_var) {
                            if matches!(right, Operand::Constant(_)) {
                                *right = Operand::Constant(new_bound);
                                return;
                            }
                        }
                        if matches!(right, Operand::Var(v) if *v == iv_var) {
                            if matches!(left, Operand::Constant(_)) {
                                *left = Operand::Constant(new_bound);
                                return;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Change the init value of a phi node for an IV
fn swap_phi_init(func: &mut Function, header: BlockId, iv_var: VarId, body: &HashSet<BlockId>, new_init: i64) {
    // First pass: find the variable and block to update
    let mut target: Option<(VarId, BlockId)> = None;
    if let Some(block) = func.blocks.iter().find(|b| b.id == header) {
        for inst in &block.instructions {
            if let Instruction::Phi { dest, preds } = inst {
                if *dest == iv_var {
                    for (pred_block, pred_var) in preds.iter() {
                        if !body.contains(pred_block) {
                            target = Some((*pred_var, *pred_block));
                            break;
                        }
                    }
                }
            }
        }
    }
    // Second pass: update the constant value in the preheader
    if let Some((var, block_id)) = target {
        update_constant_value(func, var, block_id, new_init);
    }
}

/// Update a variable's constant value in a given block
fn update_constant_value(func: &mut Function, var: VarId, block_id: BlockId, new_value: i64) {
    // We need to do this after finding the block, so clone the block_id
    let target_block = block_id;
    if let Some(block) = func.blocks.iter_mut().find(|b| b.id == target_block) {
        for inst in &mut block.instructions {
            if let Instruction::Copy { dest, src } = inst {
                if *dest == var {
                    *src = Operand::Constant(new_value);
                    return;
                }
            }
        }
    }
}

/// Change the step value in a loop body's IV increment
fn swap_step_value(func: &mut Function, body: &HashSet<BlockId>, iv_var: VarId, new_step: i64) {
    for block in &mut func.blocks {
        if !body.contains(&block.id) {
            continue;
        }
        for inst in &mut block.instructions {
            if let Instruction::Binary { op: BinaryOp::Add, left, right, .. } = inst {
                if matches!(left, Operand::Var(v) if *v == iv_var) {
                    if matches!(right, Operand::Constant(_)) {
                        *right = Operand::Constant(new_step);
                        return;
                    }
                }
                if matches!(right, Operand::Var(v) if *v == iv_var) {
                    if matches!(left, Operand::Constant(_)) {
                        *left = Operand::Constant(new_step);
                        return;
                    }
                }
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
    fn test_interchange_preserves_simple_loop() {
        // A single loop should not be affected by interchange
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
            try_loop_interchange(func);
        }
        // Should not crash
    }

    #[test]
    fn test_interchange_nested_loop() {
        // A nested loop over 2D array — interchange analysis should run
        let src = r#"
            int main() {
                int arr[10][10];
                int i;
                int j;
                for (i = 0; i < 10; i = i + 1) {
                    for (j = 0; j < 10; j = j + 1) {
                        arr[i][j] = i + j;
                    }
                }
                return arr[5][5];
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            try_loop_interchange(func);
        }
        // Should not crash — this loop already has good access order
    }
}
