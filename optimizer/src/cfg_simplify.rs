// CFG simplification: merge basic blocks and eliminate unnecessary jumps
use ir::{Function, Terminator, BlockId, Instruction};
use std::collections::{HashMap, HashSet};

/// Simplify the control flow graph by removing empty blocks (jump threading)
pub fn simplify_cfg(func: &mut Function) {
    let mut iterations = 0;
    loop {
        iterations += 1;
        if iterations > 100 {
            // Prevent infinite loops in case of bugs
            break;
        }
        
        // Run both optimizations - merge_blocks is now safe with label tracking
        let changed1 = merge_blocks(func);
        let changed2 = remove_empty_blocks(func);
        if !changed1 && !changed2 {
            break;
        }
    }
}

/// Merge blocks where a block has only one predecessor and that predecessor has only one successor
/// This runs BEFORE phi removal, so we must update phi nodes when merging.
fn merge_blocks(func: &mut Function) -> bool {
    let mut changed = false;
    
    // Build predecessor map
    let pred_map = build_predecessor_map(func);
    
    // Find blocks that can be merged
    let mut to_merge: Vec<(usize, usize)> = Vec::new();
    for i in 0..func.blocks.len() {
        let block = &func.blocks[i];
        
        // Skip blocks that are already unreachable
        if matches!(block.terminator, Terminator::Unreachable) {
            continue;
        }
        
        // Don't merge blocks that are label targets (goto destinations)
        if block.is_label_target {
            continue;
        }
        
        // Don't merge blocks with phi nodes as predecessors might be critical
        let has_phi = block.instructions.iter().any(|inst| matches!(inst, Instruction::Phi{..}));
        if has_phi {
            continue;
        }
        
        // Check if this block has exactly one successor via unconditional jump
        if let Terminator::Br(succ_id) = block.terminator {
            let succ_idx = succ_id.0;
            
            // Skip if successor is already unreachable
            if matches!(func.blocks[succ_idx].terminator, Terminator::Unreachable) {
                continue;
            }
            
            // Don't merge if successor is a label target
            if func.blocks.get(succ_idx).map_or(false, |b| b.is_label_target) {
                continue;
            }
            
            // Don't merge if successor has phi nodes - merging could break them
            let succ_has_phi = func.blocks.get(succ_idx).map_or(false, |b| {
                b.instructions.iter().any(|inst| matches!(inst, Instruction::Phi{..}))
            });
            if succ_has_phi {
                continue;
            }
            
            // Check if successor has exactly one predecessor (this block)
            if let Some(preds) = pred_map.get(&BlockId(succ_idx)) {
                if preds.len() == 1 && preds.contains(&BlockId(i)) {
                    to_merge.push((i, succ_idx));
                }
            }
        }
    }
    
    // Merge blocks (in reverse order to maintain indices)
    for &(pred_idx, succ_idx) in to_merge.iter().rev() {
        // Append successor's instructions to predecessor
        let succ_instructions = func.blocks[succ_idx].instructions.clone();
        let succ_terminator = func.blocks[succ_idx].terminator.clone();
        
        func.blocks[pred_idx].instructions.extend(succ_instructions);
        func.blocks[pred_idx].terminator = succ_terminator;
        
        // Update all references to succ_idx to point to pred_idx
        for block in func.blocks.iter_mut() {
            // Update phi nodes
            for instr in &mut block.instructions {
                if let Instruction::Phi { preds, .. } = instr {
                    for (pred_block_id, _var_id) in preds.iter_mut() {
                        if pred_block_id.0 == succ_idx {
                            *pred_block_id = BlockId(pred_idx);
                        }
                    }
                }
            }
            
            // Update terminators that reference the merged block
            match &mut block.terminator {
                Terminator::Br(target) => {
                    if target.0 == succ_idx {
                        *target = BlockId(pred_idx);
                    }
                }
                Terminator::CondBr { then_block, else_block, .. } => {
                    if then_block.0 == succ_idx {
                        *then_block = BlockId(pred_idx);
                    }
                    if else_block.0 == succ_idx {
                        *else_block = BlockId(pred_idx);
                    }
                }
                _ => {}
            }
        }
        
        // Mark successor as merged (empty it but don't remove to preserve indices)
        func.blocks[succ_idx].instructions.clear();
        func.blocks[succ_idx].terminator = Terminator::Unreachable;
        
        changed = true;
    }
    
    changed
}

/// Remove blocks that only contain an unconditional jump and redirect predecessors
fn remove_empty_blocks(func: &mut Function) -> bool {
    let mut changed = false;
    
    // Find empty blocks (no instructions, only unconditional branch)
    let mut redirect_map: HashMap<BlockId, BlockId> = HashMap::new();
    
    for i in 0..func.blocks.len() {
        let block = &func.blocks[i];
        if block.instructions.is_empty() {
            if let Terminator::Br(target) = block.terminator {
                if target.0 != i {  // Don't redirect to self
                    redirect_map.insert(BlockId(i), target);
                }
            }
        }
    }
    
    // Redirect all references to empty blocks
    if !redirect_map.is_empty() {
        for block in &mut func.blocks {
            // Apply transitive closure of redirects
            let mut new_terminator = block.terminator.clone();
            match &mut new_terminator {
                Terminator::Br(target) => {
                    let mut final_target = *target;
                    let mut visited = HashSet::new();
                    while let Some(&next) = redirect_map.get(&final_target) {
                        if visited.contains(&next) {
                            break; // Cycle detection
                        }
                        visited.insert(final_target);
                        final_target = next;
                    }
                    if final_target != *target {
                        *target = final_target;
                        changed = true;
                    }
                }
                Terminator::CondBr { then_block, else_block, .. } => {
                    let mut then_final = *then_block;
                    let mut visited = HashSet::new();
                    while let Some(&next) = redirect_map.get(&then_final) {
                        if visited.contains(&next) {
                            break;
                        }
                        visited.insert(then_final);
                        then_final = next;
                    }
                    if then_final != *then_block {
                        *then_block = then_final;
                        changed = true;
                    }
                    
                    let mut else_final = *else_block;
                    visited.clear();
                    while let Some(&next) = redirect_map.get(&else_final) {
                        if visited.contains(&next) {
                            break;
                        }
                        visited.insert(else_final);
                        else_final = next;
                    }
                    if else_final != *else_block {
                        *else_block = else_final;
                        changed = true;
                    }
                }
                _ => {}
            }
            block.terminator = new_terminator;
        }
    }
    
    changed
}

/// Build a map of block -> set of predecessor blocks
fn build_predecessor_map(func: &Function) -> HashMap<BlockId, Vec<BlockId>> {
    let mut pred_map: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    
    for block in &func.blocks {
        match block.terminator {
            Terminator::Br(target) => {
                pred_map.entry(target).or_insert_with(Vec::new).push(block.id);
            }
            Terminator::CondBr { then_block, else_block, .. } => {
                pred_map.entry(then_block).or_insert_with(Vec::new).push(block.id);
                pred_map.entry(else_block).or_insert_with(Vec::new).push(block.id);
            }
            _ => {}
        }
    }
    
    pred_map
}
