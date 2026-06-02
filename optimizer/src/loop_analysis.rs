// Loop analysis: natural loop detection for auto-vectorization
//
// Identifies natural loops in the CFG by finding back edges (edges from a node
// to a dominator) and then computing the loop body as all nodes that can reach
// the back edge source without going through the header.

use ir::{Function, Terminator, BlockId, Instruction, Operand, VarId};
use model::BinaryOp;
use std::collections::{HashMap, HashSet, VecDeque};

/// A natural loop in the CFG
#[derive(Debug, Clone)]
pub struct NaturalLoop {
    /// The loop header block (dominates all blocks in the loop)
    pub header: BlockId,
    /// The back-edge source (block that jumps back to header)
    pub latch: BlockId,
    /// All blocks in the loop body (including header and latch)
    pub body: HashSet<BlockId>,
    /// The exit block (first block outside the loop)
    pub exit: Option<BlockId>,
    /// Preheader block (unique predecessor of header from outside the loop)
    pub preheader: Option<BlockId>,
    /// Induction variable info if detected
    pub induction_var: Option<InductionVar>,
    /// Trip count if computable
    pub trip_count: Option<usize>,
}

/// Describes a simple induction variable: starts at `init`, incremented by `step` each iteration,
/// compared against `bound_operand` using `cmp_op` to determine loop exit.
#[derive(Debug, Clone)]
pub struct InductionVar {
    pub var: VarId,
    pub init: i64,
    pub step: i64,
    /// Constant bound when `bound_operand` is `Operand::Constant`.
    pub bound: i64,
    /// Loop limit (constant or SSA variable).
    pub bound_operand: Operand,
    pub cmp_op: BinaryOp,
}

/// Build a successor map from the CFG (delegates to `Function::compute_successors`)
fn build_successors(func: &Function) -> HashMap<BlockId, Vec<BlockId>> {
    func.compute_successors()
}

/// Build a predecessor map from the CFG (delegates to `Function::compute_predecessors`)
fn build_predecessors(func: &Function) -> HashMap<BlockId, Vec<BlockId>> {
    func.compute_predecessors()
}

/// Compute dominators using iterative dataflow
/// Returns a map: block → set of blocks that dominate it
fn compute_dominators(func: &Function) -> HashMap<BlockId, HashSet<BlockId>> {
    let all_blocks: HashSet<BlockId> = func.blocks.iter().map(|b| b.id).collect();
    let preds = build_predecessors(func);
    let mut doms: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

    // Entry block is dominated only by itself
    doms.insert(func.entry_block, {
        let mut s = HashSet::new();
        s.insert(func.entry_block);
        s
    });

    // All other blocks start dominated by all blocks
    for block in &func.blocks {
        if block.id != func.entry_block {
            doms.insert(block.id, all_blocks.clone());
        }
    }

    // Iterate until convergence
    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            if block.id == func.entry_block {
                continue;
            }
            let pred_list = preds.get(&block.id).cloned().unwrap_or_default();
            if pred_list.is_empty() {
                continue;
            }
            // Dom(b) = {b} ∪ ∩{Dom(p) | p ∈ preds(b)}
            let mut new_dom = all_blocks.clone();
            for p in &pred_list {
                if let Some(p_dom) = doms.get(p) {
                    new_dom = new_dom.intersection(p_dom).copied().collect();
                }
            }
            new_dom.insert(block.id);
            if new_dom != *doms.get(&block.id).unwrap_or(&HashSet::new()) {
                doms.insert(block.id, new_dom);
                changed = true;
            }
        }
    }
    doms
}

/// Find all natural loops in a function
pub fn find_loops(func: &Function) -> Vec<NaturalLoop> {
    let succs = build_successors(func);
    let preds = build_predecessors(func);
    let doms = compute_dominators(func);

    // Find back edges: edge (a → b) where b dominates a
    let mut back_edges: Vec<(BlockId, BlockId)> = Vec::new();
    for block in &func.blocks {
        for succ in succs.get(&block.id).unwrap_or(&vec![]) {
            if let Some(dom_set) = doms.get(&block.id) {
                if dom_set.contains(succ) {
                    // succ dominates block → this is a back edge
                    back_edges.push((block.id, *succ));
                }
            }
        }
    }

    let mut loops = Vec::new();
    for (latch, header) in back_edges {
        // Compute loop body: all nodes that can reach latch without going through header
        let mut body = HashSet::new();
        body.insert(header);
        body.insert(latch);

        if header != latch {
            let mut worklist = VecDeque::new();
            worklist.push_back(latch);
            while let Some(node) = worklist.pop_front() {
                for pred in preds.get(&node).unwrap_or(&vec![]) {
                    if !body.contains(pred) {
                        body.insert(*pred);
                        worklist.push_back(*pred);
                    }
                }
            }
        }

        // Find exit block(s)
        let mut exit = None;
        for &b in &body {
            for succ in succs.get(&b).unwrap_or(&vec![]) {
                if !body.contains(succ) {
                    exit = Some(*succ);
                    break;
                }
            }
            if exit.is_some() {
                break;
            }
        }

        // Find preheader: unique predecessor of header that's not in the loop
        let header_preds = preds.get(&header).unwrap_or(&vec![]).clone();
        let outside_preds: Vec<BlockId> = header_preds.into_iter().filter(|p| !body.contains(p)).collect();
        let preheader = if outside_preds.len() == 1 { Some(outside_preds[0]) } else { None };

        // Try to detect induction variable and trip count
        let induction_var = detect_induction_var(func, header, latch, &body);
        let trip_count = induction_var.as_ref().and_then(compute_trip_count);

        loops.push(NaturalLoop {
            header,
            latch,
            body,
            exit,
            preheader,
            induction_var,
            trip_count,
        });
    }

    loops
}

/// Try to detect a simple induction variable in a loop:
/// Pattern: iv starts at init (in preheader/phi), incremented by constant step,
/// compared against bound in header's terminator condition.
fn detect_induction_var(
    func: &Function,
    header: BlockId,
    _latch: BlockId,
    body: &HashSet<BlockId>,
) -> Option<InductionVar> {
    let header_block = func.blocks.iter().find(|b| b.id == header)?;

    // Look for a conditional branch in the header
    let (cond_var, then_block, else_block) = match &header_block.terminator {
        Terminator::CondBr {
            cond: Operand::Var(v),
            then_block,
            else_block,
            ..
        } => {
            (*v, *then_block, *else_block)
        }
        _ => return None,
    };

    // Determine which branch exits the loop
    let exits_on_then = !body.contains(&then_block);
    let exits_on_else = !body.contains(&else_block);
    if !exits_on_then && !exits_on_else {
        return None; // No exit from header
    }

    // Find the comparison instruction that produces cond_var
    let cmp_inst = header_block.instructions.iter().find(|inst| {
        matches!(inst, Instruction::Binary { dest, op, .. }
            if *dest == cond_var && matches!(op,
                BinaryOp::Less | BinaryOp::LessEqual |
                BinaryOp::Greater | BinaryOp::GreaterEqual |
                BinaryOp::NotEqual | BinaryOp::EqualEqual))
    })?;

    let (cmp_op, left, right) = match cmp_inst {
        Instruction::Binary { op, left, right, .. } => (op.clone(), left, right),
        _ => return None,
    };

    // One side should be the IV, other should be the bound (constant or variable).
    let (iv_var, bound_operand) = if let Operand::Var(v) = left {
        if matches!(right, Operand::Constant(_) | Operand::Var(_)) {
            (*v, right.clone())
        } else {
            return None;
        }
    } else if let Operand::Var(v) = right {
        if matches!(left, Operand::Constant(_) | Operand::Var(_)) {
            (*v, left.clone())
        } else {
            return None;
        }
    } else {
        return None;
    };

    let bound = match &bound_operand {
        Operand::Constant(c) => *c,
        _ => 0,
    };

    // If the loop continues when condition is true (exits on else),
    // the comparison is the "continue" condition.
    // If exits on then, invert the comparison semantics.
    let effective_cmp = if exits_on_then {
        // Loop exits when cmp is true, continues when false
        // We want the "exit" condition
        cmp_op.clone()
    } else {
        // Loop continues when cmp is true, exits when false
        // The exit condition is the negation
        match &cmp_op {
            BinaryOp::Less => BinaryOp::GreaterEqual,
            BinaryOp::LessEqual => BinaryOp::Greater,
            BinaryOp::Greater => BinaryOp::LessEqual,
            BinaryOp::GreaterEqual => BinaryOp::Less,
            BinaryOp::EqualEqual => BinaryOp::NotEqual,
            BinaryOp::NotEqual => BinaryOp::EqualEqual,
            _ => return None,
        }
    };

    // Look for a binary add/sub with constant step in the latch/body that updates IV
    let mut step = None;
    for &block_id in body {
        let block = func.blocks.iter().find(|b| b.id == block_id)?;
        for inst in &block.instructions {
            match inst {
                Instruction::Binary { dest, op: BinaryOp::Add, left: Operand::Var(v), right: Operand::Constant(c), .. }
                | Instruction::Binary { dest, op: BinaryOp::Add, left: Operand::Constant(c), right: Operand::Var(v), .. }
                if *v == iv_var || is_copy_of(func, *v, iv_var, body) => {
                    step = Some((*dest, *c));
                }
                Instruction::Binary { dest, op: BinaryOp::Sub, left: Operand::Var(v), right: Operand::Constant(c), .. }
                if *v == iv_var || is_copy_of(func, *v, iv_var, body) => {
                    step = Some((*dest, -*c));
                }
                _ => {}
            }
        }
    }

    let (_step_dest, step_val) = step?;

    // Try to find the initial value from a copy/constant in the preheader or phi
    let init = find_iv_init(func, iv_var, header, body)?;

    Some(InductionVar {
        var: iv_var,
        init,
        step: step_val,
        bound,
        bound_operand,
        cmp_op: effective_cmp,
    })
}

/// Check if var `a` is a copy of var `b` within the loop body
fn is_copy_of(func: &Function, a: VarId, b: VarId, body: &HashSet<BlockId>) -> bool {
    for &block_id in body {
        if let Some(block) = func.blocks.iter().find(|bl| bl.id == block_id) {
            for inst in &block.instructions {
                if let Instruction::Copy { dest, src: Operand::Var(v) } = inst {
                    if *dest == a && *v == b {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Find the initial value of the induction variable
fn find_iv_init(func: &Function, iv_var: VarId, header: BlockId, body: &HashSet<BlockId>) -> Option<i64> {
    let header_block = func.blocks.iter().find(|b| b.id == header)?;

    // Check for phi node defining iv_var
    for inst in &header_block.instructions {
        if let Instruction::Phi { dest, preds } = inst {
            if *dest == iv_var {
                // Find the predecessor that's outside the loop
                for (pred_block, pred_var) in preds {
                    if !body.contains(pred_block) {
                        // This is the init value from outside the loop
                        return find_constant_value(func, *pred_var, *pred_block);
                    }
                }
            }
        }
    }

    // Check for a copy instruction
    for inst in &header_block.instructions {
        if let Instruction::Copy { dest, src: Operand::Constant(c) } = inst {
            if *dest == iv_var {
                return Some(*c);
            }
        }
    }

    None
}

/// Try to find the constant value of a variable in a given block
fn find_constant_value(func: &Function, var: VarId, block_id: BlockId) -> Option<i64> {
    // First try: look for a constant definition of `var` in the specified block
    if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
        for inst in &block.instructions {
            match inst {
                Instruction::Copy { dest, src: Operand::Constant(c) } if *dest == var => {
                    return Some(*c);
                }
                _ => {}
            }
        }
    }

    // Second try: search ALL blocks for a constant definition of `var`
    // (the defining instruction may have been moved by optimization passes)
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Copy { dest, src: Operand::Constant(c) } if *dest == var => {
                    return Some(*c);
                }
                // Also check for a Copy from another variable, and trace through
                Instruction::Copy { dest, src: Operand::Var(src_var) } if *dest == var => {
                    // Recurse to find the constant value of src_var
                    return find_constant_in_all_blocks(func, *src_var);
                }
                _ => {}
            }
        }
    }

    // Third try: check if var itself is defined by a phi with a constant value
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Phi { dest, preds } = inst {
                if *dest == var {
                    // Check if all incoming values are the same constant
                    let mut constant_val = None;
                    let mut all_same = true;
                    for (_pred_block, pred_var) in preds {
                        if let Some(c) = find_constant_in_all_blocks(func, *pred_var) {
                            if let Some(prev) = constant_val {
                                if prev != c { all_same = false; break; }
                            }
                            constant_val = Some(c);
                        } else {
                            all_same = false;
                            break;
                        }
                    }
                    if all_same {
                        return constant_val;
                    }
                }
            }
        }
    }

    None
}

/// Find a constant value for a variable by searching all blocks
fn find_constant_in_all_blocks(func: &Function, var: VarId) -> Option<i64> {
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Copy { dest, src: Operand::Constant(c) } if *dest == var => {
                    return Some(*c);
                }
                _ => {}
            }
        }
    }
    None
}

/// Compute the trip count of a loop given its induction variable info (constant bound only).
fn compute_trip_count(iv: &InductionVar) -> Option<usize> {
    if iv.step == 0 {
        return None; // Infinite loop
    }
    if !matches!(iv.bound_operand, Operand::Constant(_)) {
        return None;
    }

    let range = iv.bound - iv.init;

    match iv.cmp_op {
        // Exit when iv >= bound (loop while iv < bound)
        BinaryOp::GreaterEqual | BinaryOp::Less => {
            if iv.step > 0 && range > 0 {
                Some(((range + iv.step - 1) / iv.step) as usize)
            } else {
                None
            }
        }
        // Exit when iv > bound (loop while iv <= bound)
        BinaryOp::Greater | BinaryOp::LessEqual => {
            if iv.step > 0 && range >= 0 {
                Some(((range + iv.step) / iv.step) as usize)
            } else {
                None
            }
        }
        // Exit when iv == bound
        BinaryOp::EqualEqual => {
            if iv.step != 0 && range % iv.step == 0 && (range / iv.step) >= 0 {
                Some((range / iv.step) as usize)
            } else {
                None
            }
        }
        // Exit when iv != bound (loop while iv == bound) — unusual
        BinaryOp::NotEqual => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{BasicBlock, Terminator, Instruction, Operand, VarId, BlockId};

    fn make_simple_loop_func() -> Function {
        // Create a simple loop:
        //   block0 (preheader): i = 0; goto block1
        //   block1 (header): if i < 10 goto block2 else goto block3
        //   block2 (body): i = i + 1; goto block1
        //   block3 (exit): return
        Function {
            name: "test_loop".to_string(),
            return_type: model::Type::Int,
            params: vec![],
            entry_block: BlockId(0),
            var_types: HashMap::new(),
            attributes: vec![],
            is_static: false,
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![
                        Instruction::Copy { dest: VarId(0), src: Operand::Constant(0) },
                    ],
                    terminator: Terminator::Br(BlockId(1)),
                    is_label_target: false,
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![
                        Instruction::Phi {
                            dest: VarId(1),
                            preds: vec![(BlockId(0), VarId(0)), (BlockId(2), VarId(3))],
                        },
                        Instruction::Binary {
                            dest: VarId(2),
                            op: BinaryOp::Less,
                            left: Operand::Var(VarId(1)),
                            right: Operand::Constant(10),
                        },
                    ],
                    terminator: Terminator::cond_br(
                        Operand::Var(VarId(2)),
                        BlockId(2),
                        BlockId(3),
                    ),
                    is_label_target: false,
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![
                        Instruction::Binary {
                            dest: VarId(3),
                            op: BinaryOp::Add,
                            left: Operand::Var(VarId(1)),
                            right: Operand::Constant(1),
                        },
                    ],
                    terminator: Terminator::Br(BlockId(1)),
                    is_label_target: false,
                },
                BasicBlock {
                    id: BlockId(3),
                    instructions: vec![],
                    terminator: Terminator::Ret(Some(Operand::Constant(0))),
                    is_label_target: false,
                },
            ],
        }
    }

    #[test]
    fn test_find_loops() {
        let func = make_simple_loop_func();
        let loops = find_loops(&func);
        assert_eq!(loops.len(), 1, "Should find exactly one loop");
        let lp = &loops[0];
        assert_eq!(lp.header, BlockId(1));
        assert!(lp.body.contains(&BlockId(1)));
        assert!(lp.body.contains(&BlockId(2)));
        assert!(!lp.body.contains(&BlockId(0)));
        assert!(!lp.body.contains(&BlockId(3)));
    }

    #[test]
    fn test_loop_exit() {
        let func = make_simple_loop_func();
        let loops = find_loops(&func);
        assert_eq!(loops[0].exit, Some(BlockId(3)));
    }

    #[test]
    fn test_preheader() {
        let func = make_simple_loop_func();
        let loops = find_loops(&func);
        assert_eq!(loops[0].preheader, Some(BlockId(0)));
    }

    #[test]
    fn test_induction_var() {
        let func = make_simple_loop_func();
        let loops = find_loops(&func);
        let iv = loops[0].induction_var.as_ref().expect("Should detect induction variable");
        assert_eq!(iv.init, 0);
        assert_eq!(iv.step, 1);
        assert_eq!(iv.bound, 10);
    }

    #[test]
    fn test_trip_count() {
        let func = make_simple_loop_func();
        let loops = find_loops(&func);
        assert_eq!(loops[0].trip_count, Some(10));
    }

    #[test]
    fn test_no_loops() {
        let func = Function {
            name: "no_loop".to_string(),
            return_type: model::Type::Int,
            params: vec![],
            entry_block: BlockId(0),
            var_types: HashMap::new(),
            attributes: vec![],
            is_static: false,
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Ret(Some(Operand::Constant(42))),
                    is_label_target: false,
                },
            ],
        };
        let loops = find_loops(&func);
        assert!(loops.is_empty());
    }
}
