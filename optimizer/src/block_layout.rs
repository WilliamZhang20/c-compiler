// Block Layout Optimization for I-cache Locality
//
// Reorders basic blocks to improve instruction cache performance by:
// 1. Placing loop headers followed immediately by loop bodies (keeps hot loops tight)
// 2. Placing fall-through targets immediately after conditional branches
// 3. Keeping the loop exit block after the loop (cold path deferred)
//
// This is a standard compiler optimization that reduces I-cache misses by
// ensuring the most likely execution path is laid out sequentially in memory.

use ir::{Function, Terminator, BlockId};
use std::collections::{HashSet, VecDeque};
use crate::loop_analysis;

/// Optimize block layout for better I-cache locality
pub fn optimize_block_layout(func: &mut Function) {
    if func.blocks.len() <= 2 {
        return; // Nothing to optimize
    }

    // Detect loops to identify hot blocks
    let loops = loop_analysis::find_loops(func);
    let mut loop_blocks: HashSet<BlockId> = HashSet::new();
    let mut loop_headers: HashSet<BlockId> = HashSet::new();

    for lp in &loops {
        loop_headers.insert(lp.header);
        for &b in &lp.body {
            loop_blocks.insert(b);
        }
    }

    // Build successor map
    let succ_map: std::collections::HashMap<BlockId, Vec<BlockId>> = func.blocks.iter()
        .map(|b| {
            let succs = match &b.terminator {
                Terminator::Br(t) => vec![*t],
                Terminator::CondBr { then_block, else_block, .. } => vec![*then_block, *else_block],
                _ => vec![],
            };
            (b.id, succs)
        })
        .collect();

    // Layout blocks using a modified DFS that prioritizes:
    // 1. Fall-through targets (then_block for CondBr, target for Br)
    // 2. Loop body blocks before exit blocks
    // 3. Hot (loop) blocks before cold blocks
    let mut ordered: Vec<BlockId> = Vec::with_capacity(func.blocks.len());
    let mut visited: HashSet<BlockId> = HashSet::new();
    let mut worklist: VecDeque<BlockId> = VecDeque::new();

    // Start with entry block
    worklist.push_back(func.entry_block);

    while let Some(block_id) = worklist.pop_front() {
        if visited.contains(&block_id) {
            continue;
        }
        visited.insert(block_id);
        ordered.push(block_id);

        // Get successors in priority order
        if let Some(succs) = succ_map.get(&block_id) {
            let block = func.blocks.iter().find(|b| b.id == block_id);
            match block.map(|b| &b.terminator) {
                Some(Terminator::Br(target)) => {
                    // Unconditional: place target next
                    if !visited.contains(target) {
                        worklist.push_front(*target);
                    }
                }
                Some(Terminator::CondBr {
                    then_block,
                    else_block,
                    hint,
                    ..
                }) => {
                    use ir::BranchHint;
                    if *hint == BranchHint::LikelyThen {
                        if !visited.contains(then_block) {
                            worklist.push_front(*then_block);
                        }
                        if !visited.contains(else_block) {
                            worklist.push_back(*else_block);
                        }
                    } else if *hint == BranchHint::LikelyElse {
                        if !visited.contains(else_block) {
                            worklist.push_front(*else_block);
                        }
                        if !visited.contains(then_block) {
                            worklist.push_back(*then_block);
                        }
                    } else {
                    // For conditional branches in loop headers:
                    // Place the loop body block next (fall-through),
                    // and the exit block later.
                    let then_is_loop = loop_blocks.contains(then_block);
                    let else_is_loop = loop_blocks.contains(else_block);

                    if then_is_loop && !else_is_loop {
                        // then is loop body, else is exit — place then first
                        if !visited.contains(then_block) {
                            worklist.push_front(*then_block);
                        }
                        if !visited.contains(else_block) {
                            worklist.push_back(*else_block);
                        }
                    } else if else_is_loop && !then_is_loop {
                        // else is loop body, then is exit — place else first
                        if !visited.contains(else_block) {
                            worklist.push_front(*else_block);
                        }
                        if !visited.contains(then_block) {
                            worklist.push_back(*then_block);
                        }
                    } else {
                        // Both or neither in loop — default: then first
                        if !visited.contains(then_block) {
                            worklist.push_front(*then_block);
                        }
                        if !visited.contains(else_block) {
                            // Push exit/cold path to back
                            worklist.push_back(*else_block);
                        }
                    }
                    }
                }
                _ => {
                    // Ret/Unreachable: no successors
                    for s in succs {
                        if !visited.contains(s) {
                            worklist.push_back(*s);
                        }
                    }
                }
            }
        }
    }

    // Add any unreachable blocks that weren't visited
    for block in &func.blocks {
        if !visited.contains(&block.id) {
            ordered.push(block.id);
        }
    }

    // Reorder blocks according to the layout
    if ordered.len() == func.blocks.len() {
        let mut new_blocks = Vec::with_capacity(func.blocks.len());
        for &block_id in &ordered {
            if let Some(idx) = func.blocks.iter().position(|b| b.id == block_id) {
                new_blocks.push(func.blocks[idx].clone());
            }
        }
        func.blocks = new_blocks;
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
    fn test_block_layout_preserves_correctness() {
        let src = r#"
            int main() {
                int sum = 0;
                int i;
                for (i = 0; i < 100; i = i + 1) {
                    if (i % 2 == 0) {
                        sum = sum + i;
                    }
                }
                return sum % 256;
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            optimize_block_layout(func);
        }
        // Should not crash and basic block order should be valid
        assert!(!prog.functions[0].blocks.is_empty());
    }

    #[test]
    fn test_block_layout_loop_first() {
        let src = r#"
            int main() {
                int i;
                int sum = 0;
                for (i = 0; i < 10; i = i + 1) {
                    sum = sum + i;
                }
                return sum;
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            optimize_block_layout(func);
        }
        // Entry block should still be first
        assert_eq!(prog.functions[0].blocks[0].id, prog.functions[0].entry_block);
    }
}
