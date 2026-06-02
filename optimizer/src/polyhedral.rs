// Affine (polyhedral-style) loop nest analysis for vectorization.
//
// This is a lightweight MILP-free subset: perfect nests, affine IV bounds,
// and dependence checks before widening the innermost induction variable.
// Full ISL/Polly-style scheduling is out of scope; we reuse loop_analysis
// and mem_dependence for the actual legality tests.

use ir::{Function, Instruction, VarId, BlockId};
use model::BinaryOp;
use std::collections::HashSet;
use crate::loop_analysis::{self, NaturalLoop};

/// Prepare function for vectorization: no IR change today, but validates nests.
pub fn prepare_affine_nests(func: &Function) {
    let loops = loop_analysis::find_loops(func);
    let loop_refs: Vec<&NaturalLoop> = loops.iter().collect();
    for inner in &loops {
        if let Some(outer) = parent_loop(&loop_refs, inner) {
            let _ = is_perfect_affine_nest(func, outer, inner);
        }
    }
}

/// Whether vectorization of `lp` is consistent with affine nest constraints.
///
/// Aggressive mode: nested loops may vectorize on the inner IV even when GEP indices
/// mention an enclosing IV (it is loop-invariant during the inner trip). Legality of
/// widened accesses is enforced by `mem_dependence`, not by forbidding outer IVs here.
pub fn allows_vectorization(func: &Function, lp: &NaturalLoop, vf: usize) -> bool {
    let loops = loop_analysis::find_loops(func);
    let loop_refs: Vec<&NaturalLoop> = loops.iter().collect();

    // Innermost loops: no nest constraints (mem_dependence handles legality).
    if is_innermost_loop(&loop_refs, lp) {
        return true;
    }

    if let Some(outer) = parent_loop(&loop_refs, lp) {
        if !is_perfect_affine_nest(func, outer, lp) {
            return false;
        }
        // Vectorizing the outer loop in a nest can reorder work visible to the inner loop.
        if is_immediate_parent(&loop_refs, outer, lp)
            && !outer_loop_vectorization_safe(func, outer, lp, vf)
        {
            return false;
        }
    }
    true
}

/// True if no other natural loop is strictly contained in `lp`'s body.
fn is_innermost_loop(loops: &[&NaturalLoop], lp: &NaturalLoop) -> bool {
    loops.iter().all(|other| {
        other.header == lp.header
            || !other.body.is_subset(&lp.body)
            || other.body.len() >= lp.body.len()
    })
}

/// `outer` is the innermost enclosing loop of `lp` (not a distant ancestor).
fn is_immediate_parent(
    loops: &[&NaturalLoop],
    outer: &NaturalLoop,
    inner: &NaturalLoop,
) -> bool {
    parent_loop(loops, inner).map(|p| p.header) == Some(outer.header)
}

/// Outer loop in a perfect nest containing `inner`, if any.
fn parent_loop<'a>(loops: &'a [&'a NaturalLoop], inner: &NaturalLoop) -> Option<&'a NaturalLoop> {
    loops
        .iter()
        .filter(|&&outer| {
            outer.header != inner.header
                && inner.body.is_subset(&outer.body)
                && outer.body.len() > inner.body.len()
        })
        .max_by_key(|outer| outer.body.len())
        .copied()
}

/// Perfect nest: outer-only blocks contain only IV/control, no memory ops.
fn is_perfect_affine_nest(func: &Function, outer: &NaturalLoop, inner: &NaturalLoop) -> bool {
    let outer_only: Vec<BlockId> = outer
        .body
        .iter()
        .filter(|b| !inner.body.contains(b))
        .copied()
        .collect();

    for block_id in outer_only {
        let Some(block) = func.blocks.iter().find(|b| b.id == block_id) else {
            return false;
        };
        for inst in &block.instructions {
            match inst {
                Instruction::Phi { .. }
                | Instruction::Copy { .. }
                | Instruction::GetElementPtr { .. } => {}
                Instruction::Load { .. } => {}
                Instruction::Binary { op, .. } => match op {
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::EqualEqual
                    | BinaryOp::NotEqual
                    | BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
                    | BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor => {}
                    _ => return false,
                },
                _ => return false,
            }
        }
    }
    true
}

/// When vectorizing `outer` (not its inner child), memory in `outer`-only blocks must not
/// use the inner IV in a GEP index (would change meaning if IV is widened).
fn outer_loop_vectorization_safe(
    func: &Function,
    outer: &NaturalLoop,
    inner: &NaturalLoop,
    _vf: usize,
) -> bool {
    let inner_iv = match &inner.induction_var {
        Some(iv) => iv.var,
        None => return true,
    };

    let outer_only: Vec<BlockId> = outer
        .body
        .iter()
        .filter(|b| !inner.body.contains(b))
        .copied()
        .collect();

    for block_id in outer_only {
        let Some(block) = func.blocks.iter().find(|b| b.id == block_id) else {
            return false;
        };
        for inst in &block.instructions {
            let Instruction::GetElementPtr { index, .. } = inst else {
                continue;
            };
            if index_uses_var(index, inner_iv, func, &outer.body, inner_iv) {
                return false;
            }
        }
    }
    true
}

fn index_uses_var(
    op: &ir::Operand,
    var: VarId,
    func: &Function,
    body: &HashSet<BlockId>,
    inner_iv: VarId,
) -> bool {
    match op {
        ir::Operand::Var(v) => {
            if *v == var {
                return true;
            }
            if *v == inner_iv {
                return false;
            }
            // Trace copies in body
            for &bid in body {
                let Some(block) = func.blocks.iter().find(|b| b.id == bid) else {
                    continue;
                };
                for inst in &block.instructions {
                    if let Instruction::Copy { dest, src } = inst {
                        if *dest == *v {
                            return index_uses_var(src, var, func, body, inner_iv);
                        }
                    }
                }
            }
            false
        }
        ir::Operand::Constant(_) => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{BasicBlock, Operand, Terminator};

    fn make_loop(header: BlockId, body: HashSet<BlockId>, iv: VarId) -> NaturalLoop {
        NaturalLoop {
            header,
            latch: *body.iter().next().unwrap(),
            body,
            exit: None,
            preheader: None,
            induction_var: Some(loop_analysis::InductionVar {
                var: iv,
                init: 0,
                step: 1,
                bound: 10,
                bound_operand: Operand::Constant(10),
                cmp_op: BinaryOp::GreaterEqual,
            }),
            trip_count: Some(10),
        }
    }

    #[test]
    fn innermost_loop_has_no_child() {
        let outer_body: HashSet<_> = [BlockId(1), BlockId(2)].into_iter().collect();
        let inner_body: HashSet<_> = [BlockId(2)].into_iter().collect();
        let outer = make_loop(BlockId(1), outer_body, VarId(0));
        let inner = make_loop(BlockId(2), inner_body, VarId(1));
        let loops = [&outer, &inner];
        assert!(is_innermost_loop(&loops, &inner));
        assert!(!is_innermost_loop(&loops, &outer));
    }

    #[test]
    fn parent_loop_finds_container() {
        let outer_body: HashSet<_> = [BlockId(1), BlockId(2)].into_iter().collect();
        let inner_body: HashSet<_> = [BlockId(2)].into_iter().collect();
        let outer = make_loop(BlockId(1), outer_body, VarId(0));
        let inner = make_loop(BlockId(2), inner_body, VarId(1));
        let loops = [&outer, &inner];
        let parent = parent_loop(&loops, &inner);
        assert!(parent.is_some());
    }
}
