// Liveness analysis for register allocation
// Extracted from regalloc.rs: compute_live_intervals, visit_operands

use ir::{VarId, Function as IrFunction, Instruction as IrInstruction, Terminator as IrTerminator, Operand};
use std::collections::{HashMap, HashSet};
use crate::regalloc::LiveInterval;

pub fn compute_live_intervals(func: &IrFunction) -> Vec<LiveInterval> {
    let mut alloca_vars: HashSet<VarId> = HashSet::new();
    
    // First pass: identify alloca variables (pointers that shouldn't be in registers)
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Alloca { dest, .. } = inst {
                alloca_vars.insert(*dest);
            }
        }
    }
    
    // Build block index: BlockId -> index into func.blocks
    let block_index: HashMap<ir::BlockId, usize> = func.blocks.iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect();
    
    // Compute per-block use/def sets and successors
    let num_blocks = func.blocks.len();
    let mut block_use: Vec<HashSet<VarId>> = vec![HashSet::new(); num_blocks];
    let mut block_def: Vec<HashSet<VarId>> = vec![HashSet::new(); num_blocks];
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); num_blocks];
    let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); num_blocks];
    
    for (bi, block) in func.blocks.iter().enumerate() {
        // Process instructions: use before def matters
        for inst in &block.instructions {
            // Record uses (variables used before being defined in this block)
            inst.for_each_use(|var| {
                if !alloca_vars.contains(&var) && !block_def[bi].contains(&var) {
                    block_use[bi].insert(var);
                }
            });
            
            // Record defs (using accessor)
            if let Some(var) = inst.dest() {
                if !alloca_vars.contains(&var) {
                    block_def[bi].insert(var);
                }
            }
        }
        
        // Handle terminator uses
        match &block.terminator {
            IrTerminator::CondBr { cond, then_block, else_block } => {
                if let Operand::Var(v) = cond {
                    if !alloca_vars.contains(v) && !block_def[bi].contains(v) {
                        block_use[bi].insert(*v);
                    }
                }
                if let Some(&ti) = block_index.get(then_block) {
                    successors[bi].push(ti);
                    predecessors[ti].push(bi);
                }
                if let Some(&ei) = block_index.get(else_block) {
                    successors[bi].push(ei);
                    predecessors[ei].push(bi);
                }
            }
            IrTerminator::Br(target) => {
                if let Some(&ti) = block_index.get(target) {
                    successors[bi].push(ti);
                    predecessors[ti].push(bi);
                }
            }
            IrTerminator::Ret(Some(Operand::Var(v))) => {
                if !alloca_vars.contains(v) && !block_def[bi].contains(v) {
                    block_use[bi].insert(*v);
                }
            }
            _ => {}
        }
    }
    
    // Iterative dataflow liveness analysis
    // live_in(B) = use(B) ∪ (live_out(B) - def(B))
    // live_out(B) = ∪ live_in(S) for all successors S of B
    let mut live_in: Vec<HashSet<VarId>> = vec![HashSet::new(); num_blocks];
    let mut live_out: Vec<HashSet<VarId>> = vec![HashSet::new(); num_blocks];
    
    let mut changed = true;
    while changed {
        changed = false;
        // Process blocks in reverse order for faster convergence
        for bi in (0..num_blocks).rev() {
            // live_out(B) = ∪ live_in(S) for all successors S
            let mut new_live_out = HashSet::new();
            for &si in &successors[bi] {
                for v in &live_in[si] {
                    new_live_out.insert(*v);
                }
            }
            
            // live_in(B) = use(B) ∪ (live_out(B) - def(B))
            let mut new_live_in = block_use[bi].clone();
            for v in &new_live_out {
                if !block_def[bi].contains(v) {
                    new_live_in.insert(*v);
                }
            }
            
            if new_live_in != live_in[bi] || new_live_out != live_out[bi] {
                changed = true;
                live_in[bi] = new_live_in;
                live_out[bi] = new_live_out;
            }
        }
    }
    
    // Now assign positions and compute intervals using both position-based
    // local info and CFG-based liveness
    let mut intervals: HashMap<VarId, (usize, usize)> = HashMap::new();
    
    // Compute position range for each block
    let mut block_start_pos: Vec<usize> = Vec::with_capacity(num_blocks);
    let mut block_end_pos: Vec<usize> = Vec::with_capacity(num_blocks);
    let mut position = 0;
    for block in &func.blocks {
        block_start_pos.push(position);
        position += block.instructions.len();
        position += 1; // terminator
        block_end_pos.push(position - 1);
    }
    
    // First: record def/use positions within each block (local precision)
    position = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            // Record defs using accessor
            if let Some(var) = inst.dest() {
                if !alloca_vars.contains(&var) {
                    let entry = intervals.entry(var).or_insert((position, position));
                    if position < entry.0 { entry.0 = position; }
                    if position > entry.1 { entry.1 = position; }
                }
            }
            
            // Record uses using accessor
            inst.for_each_use(|var| {
                if !alloca_vars.contains(&var) {
                    let entry = intervals.entry(var).or_insert((position, position));
                    if position < entry.0 { entry.0 = position; }
                    if position > entry.1 { entry.1 = position; }
                }
            });
            
            position += 1;
        }
        
        // Handle terminator operands
        match &block.terminator {
            IrTerminator::CondBr { cond, .. } => {
                if let Operand::Var(v) = cond {
                    if !alloca_vars.contains(v) {
                        let entry = intervals.entry(*v).or_insert((position, position));
                        if position < entry.0 { entry.0 = position; }
                        if position > entry.1 { entry.1 = position; }
                    }
                }
            }
            IrTerminator::Ret(Some(Operand::Var(v))) => {
                if !alloca_vars.contains(v) {
                    let entry = intervals.entry(*v).or_insert((position, position));
                    if position < entry.0 { entry.0 = position; }
                    if position > entry.1 { entry.1 = position; }
                }
            }
            _ => {}
        }
        position += 1;
    }
    
    // Second: extend intervals for variables that are live-in or live-out of blocks
    // If a variable is live-in to a block, it must be live from the start of that block
    // If a variable is live-out of a block, it must be live through the end of that block
    for bi in 0..num_blocks {
        let bstart = block_start_pos[bi];
        let bend = block_end_pos[bi];
        
        for v in &live_in[bi] {
            let entry = intervals.entry(*v).or_insert((bstart, bstart));
            if bstart < entry.0 { entry.0 = bstart; }
            if bend > entry.1 { entry.1 = bend; }
        }
        
        for v in &live_out[bi] {
            let entry = intervals.entry(*v).or_insert((bstart, bstart));
            if bstart < entry.0 { entry.0 = bstart; }
            if bend > entry.1 { entry.1 = bend; }
        }
    }
    
    intervals.into_iter()
        .map(|(var, (start, end))| LiveInterval {
            var,
            start,
            end,
            reg: None,
            spill_slot: None,
        })
        .collect()
}

/// Visit all VarIds used by an instruction. Delegates to the centralized
/// `Instruction::for_each_use` accessor on the IR type.
pub fn visit_operands<F>(inst: &IrInstruction, mut f: F)
where F: FnMut(VarId) {
    inst.for_each_use(&mut f);
}
