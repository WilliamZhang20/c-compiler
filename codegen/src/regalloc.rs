// Register allocation with graph coloring
use ir::{VarId, Function as IrFunction, Instruction as IrInstruction, Terminator as IrTerminator, Operand};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalReg {
    Rax, Rcx, Rdx, Rbx, Rsi, Rdi, R8, R9, R10, R11, R12, R13, R14, R15,
}

impl PhysicalReg {
    pub fn to_x86(&self) -> crate::x86::X86Reg {
        use crate::x86::X86Reg;
        match self {
            Self::Rax => X86Reg::Rax,
            Self::Rcx => X86Reg::Rcx,
            Self::Rdx => X86Reg::Rdx,
            Self::Rbx => X86Reg::Rbx,
            Self::Rsi => X86Reg::Rsi,
            Self::Rdi => X86Reg::Rdi,
            Self::R8 => X86Reg::R8,
            Self::R9 => X86Reg::R9,
            Self::R10 => X86Reg::R10,
            Self::R11 => X86Reg::R11,
            Self::R12 => X86Reg::R12,
            Self::R13 => X86Reg::R13,
            Self::R14 => X86Reg::R14,
            Self::R15 => X86Reg::R15,
        }
    }
    
    // Caller-saved registers (volatile) - platform-specific
    pub fn caller_saved(target: &model::TargetConfig) -> Vec<PhysicalReg> {
        match target.calling_convention {
            model::CallingConvention::WindowsX64 => {
                vec![Self::Rax, Self::Rcx, Self::Rdx, Self::R8, Self::R9, Self::R10, Self::R11]
            }
            model::CallingConvention::SystemV => {
                // System V: Rax, Rcx, Rdx, Rsi, Rdi, R8-R11 are caller-saved
                vec![Self::Rax, Self::Rcx, Self::Rdx, Self::Rsi, Self::Rdi, Self::R8, Self::R9, Self::R10, Self::R11]
            }
        }
    }
    
    // Callee-saved registers (non-volatile) - platform-specific
    pub fn callee_saved(target: &model::TargetConfig) -> Vec<PhysicalReg> {
        match target.calling_convention {
            model::CallingConvention::WindowsX64 => {
                // Windows x64: Rbx, Rsi, Rdi, Rbp, R12-R15 are callee-saved
                vec![Self::Rbx, Self::Rsi, Self::Rdi, Self::R12, Self::R13, Self::R14, Self::R15]
            }
            model::CallingConvention::SystemV => {
                // System V: Rbx, Rbp, R12-R15 are callee-saved (Rsi, Rdi are caller-saved!)
                vec![Self::Rbx, Self::R12, Self::R13, Self::R14, Self::R15]
            }
        }
    }
    
    // All allocatable registers - platform-specific
    // Exclude Rax, Rcx, Rdx as they are used as scratch registers in codegen
    // Exclude R10, R11 as they are used as scratch for address loading
    pub fn allocatable(target: &model::TargetConfig) -> Vec<PhysicalReg> {
        match target.calling_convention {
            model::CallingConvention::WindowsX64 => {
                vec![
                    Self::Rbx, Self::Rsi, Self::Rdi, 
                    Self::R8, Self::R9, 
                    Self::R12, Self::R13, Self::R14, Self::R15,
                ]
            }
            model::CallingConvention::SystemV => {
                // System V: Can use more registers since Rsi, Rdi are caller-saved
                vec![
                    Self::Rbx, Self::Rsi, Self::Rdi, 
                    Self::R8, Self::R9, 
                    Self::R12, Self::R13, Self::R14, Self::R15,
                ]
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveInterval {
    pub var: VarId,
    pub start: usize,
    pub end: usize,
    pub reg: Option<PhysicalReg>,
    #[allow(dead_code)] // Reserved for future spilling implementation
    pub spill_slot: Option<i32>,
}

/// allocate_registers performs graph-coloring register allocation with copy coalescing
pub fn allocate_registers(func: &IrFunction, target: &model::TargetConfig) -> HashMap<VarId, PhysicalReg> {
    // 1. Compute live intervals for each variable
    let mut intervals = compute_live_intervals(func);
    
    // Sort intervals by var ID to be deterministic
    intervals.sort_by_key(|i| i.var);
    
    // 2. Build interference graph
    let interference = build_interference_graph(&intervals);
    
    // 3. Collect copy hints for coalescing (maps dest -> src for Copy instructions)
    let copy_hints = collect_copy_hints(func);
    
    // 4. Build parameter register hints to prefer incoming registers
    let param_hints = build_param_hints(func, target);
    
    // 5. Determine if we should allow callee-saved registers
    // For small functions, avoid callee-saved to eliminate push/pop overhead
    let use_callee_saved = should_use_callee_saved(func, target);
    
    // 6. Identify variables that are live across function calls
    // These variables cannot use caller-saved registers
    let live_across_call = compute_live_across_call(&intervals, func);
    
    // 7. Graph coloring with copy coalescing and parameter hints
    color_graph(&mut intervals, &interference, &copy_hints, &param_hints, use_callee_saved, &live_across_call, target);
    
    // 8. Build result map
    let mut reg_alloc = HashMap::new();
    for interval in &intervals {
        if let Some(reg) = interval.reg {
            reg_alloc.insert(interval.var, reg);
        }
    }
    
    reg_alloc
}

fn should_use_callee_saved(func: &IrFunction, target: &model::TargetConfig) -> bool {
    // Heuristic: for small functions with few blocks and instructions,
    // prefer to spill to stack rather than use callee-saved registers
    // This avoids the push/pop overhead which can be significant
    
    let num_blocks = func.blocks.len();
    let num_instructions: usize = func.blocks.iter()
        .map(|b| b.instructions.len())
        .sum();
    
    // Allow callee-saved only for larger functions (>5 blocks or >30 instructions)
    // Note: target is available for future platform-specific heuristics
    let _ = target; // Silence unused warning
    num_blocks > 5 || num_instructions > 30
}

fn compute_live_intervals(func: &IrFunction) -> Vec<LiveInterval> {
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
            visit_operands(inst, |var| {
                if !alloca_vars.contains(&var) && !block_def[bi].contains(&var) {
                    block_use[bi].insert(var);
                }
            });
            
            // Record defs
            let def_var = match inst {
                IrInstruction::Binary { dest, .. } |
                IrInstruction::FloatBinary { dest, .. } |
                IrInstruction::Unary { dest, .. } |
                IrInstruction::FloatUnary { dest, .. } |
                IrInstruction::Phi { dest, .. } |
                IrInstruction::Copy { dest, .. } |
                IrInstruction::Cast { dest, .. } |
                IrInstruction::Load { dest, .. } |
                IrInstruction::GetElementPtr { dest, .. } => Some(*dest),
                IrInstruction::Call { dest, .. } => *dest,
                IrInstruction::IndirectCall { dest, .. } => *dest,
                IrInstruction::InlineAsm { outputs, .. } => {
                    outputs.first().copied()
                }
                IrInstruction::VaArg { dest, .. } => Some(*dest),
                IrInstruction::Alloca { .. } | IrInstruction::Store { .. }
                | IrInstruction::VaStart { .. } | IrInstruction::VaEnd { .. } | IrInstruction::VaCopy { .. } => None,
            };
            if let Some(var) = def_var {
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
            let def_var = match inst {
                IrInstruction::Binary { dest, .. } |
                IrInstruction::FloatBinary { dest, .. } |
                IrInstruction::Unary { dest, .. } |
                IrInstruction::FloatUnary { dest, .. } |
                IrInstruction::Phi { dest, .. } |
                IrInstruction::Copy { dest, .. } |
                IrInstruction::Cast { dest, .. } |
                IrInstruction::Load { dest, .. } |
                IrInstruction::GetElementPtr { dest, .. } => Some(*dest),
                IrInstruction::Call { dest, .. } => *dest,
                IrInstruction::IndirectCall { dest, .. } => *dest,
                IrInstruction::InlineAsm { outputs, .. } => {
                    outputs.first().copied()
                }
                IrInstruction::VaArg { dest, .. } => Some(*dest),
                IrInstruction::Alloca { .. } | IrInstruction::Store { .. }
                | IrInstruction::VaStart { .. } | IrInstruction::VaEnd { .. } | IrInstruction::VaCopy { .. } => None,
            };
            
            if let Some(var) = def_var {
                if !alloca_vars.contains(&var) {
                    let entry = intervals.entry(var).or_insert((position, position));
                    if position < entry.0 { entry.0 = position; }
                    if position > entry.1 { entry.1 = position; }
                }
            }
            
            visit_operands(inst, |var| {
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

fn visit_operands<F>(inst: &IrInstruction, mut f: F)
where F: FnMut(VarId) {
    match inst {
        IrInstruction::Binary { left, right, .. } => {
            if let Operand::Var(v) = left { f(*v); }
            if let Operand::Var(v) = right { f(*v); }
        }
        IrInstruction::FloatBinary { left, right, .. } => {
            if let Operand::Var(v) = left { f(*v); }
            if let Operand::Var(v) = right { f(*v); }
        }
        IrInstruction::Unary { src, .. } => {
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::FloatUnary { src, .. } => {
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::Copy { src, .. } => {
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::Cast { src, .. } => {
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::Load { addr, .. } => {
            if let Operand::Var(v) = addr { f(*v); }
        }
        IrInstruction::Store { addr, src, .. } => {
            if let Operand::Var(v) = addr { f(*v); }
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::GetElementPtr { base, index, .. } => {
            if let Operand::Var(v) = base { f(*v); }
            if let Operand::Var(v) = index { f(*v); }
        }
        IrInstruction::Call { args, .. } => {
            for arg in args {
                if let Operand::Var(v) = arg { f(*v); }
            }
        }
        IrInstruction::IndirectCall { func_ptr, args, .. } => {
            if let Operand::Var(v) = func_ptr { f(*v); }
            for arg in args {
                if let Operand::Var(v) = arg { f(*v); }
            }
        }
        IrInstruction::Phi { preds, .. } => {
            for (_, v) in preds {
                f(*v);
            }
        }
        IrInstruction::InlineAsm { inputs, .. } => {
            for input in inputs {
                if let Operand::Var(v) = input { f(*v); }
            }
        }
        IrInstruction::VaStart { list, .. } => {
            if let Operand::Var(v) = list { f(*v); }
        }
        IrInstruction::VaEnd { list } => {
            if let Operand::Var(v) = list { f(*v); }
        }
        IrInstruction::VaCopy { dest, src } => {
            if let Operand::Var(v) = dest { f(*v); }
            if let Operand::Var(v) = src { f(*v); }
        }
        IrInstruction::VaArg { list, .. } => {
            if let Operand::Var(v) = list { f(*v); }
        }
        IrInstruction::Alloca { .. } => {}
    }
}

fn collect_copy_hints(func: &IrFunction) -> HashMap<VarId, VarId> {
    let mut hints = HashMap::new();
    
    // Collect copy instructions where src is a Var (dest = src)
    // These are candidates for coalescing (assigning same register to both)
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Copy { dest, src } = inst {
                if let Operand::Var(src_var) = src {
                    hints.insert(*dest, *src_var);
                }
            }
        }
    }
    
    hints
}

/// Build hints for parameter variables to prefer their incoming registers
fn build_param_hints(func: &IrFunction, target: &model::TargetConfig) -> HashMap<VarId, PhysicalReg> {
    let mut hints = HashMap::new();
    
    // Map System V parameter registers to PhysicalReg
    let param_physical_regs: Vec<PhysicalReg> = match target.calling_convention {
        model::CallingConvention::WindowsX64 => {
            vec![PhysicalReg::Rcx, PhysicalReg::Rdx, PhysicalReg::R8, PhysicalReg::R9]
        }
        model::CallingConvention::SystemV => {
            vec![PhysicalReg::Rdi, PhysicalReg::Rsi, PhysicalReg::Rdx, 
                 PhysicalReg::Rcx, PhysicalReg::R8, PhysicalReg::R9]
        }
    };
    
    // Hint each parameter to prefer its incoming register
    for (i, (_, var_id)) in func.params.iter().enumerate() {
        if i < param_physical_regs.len() {
            hints.insert(*var_id, param_physical_regs[i]);
        }
    }
    
    hints
}

fn build_interference_graph(intervals: &[LiveInterval]) -> HashMap<VarId, HashSet<VarId>> {
    let mut graph: HashMap<VarId, HashSet<VarId>> = HashMap::new();
    
    // Two variables interfere if their live intervals overlap
    for i in 0..intervals.len() {
        for j in (i + 1)..intervals.len() {
            let int1 = &intervals[i];
            let int2 = &intervals[j];
            
            // Check if intervals overlap
            if int1.start <= int2.end && int2.start <= int1.end {
                graph.entry(int1.var).or_insert_with(HashSet::new).insert(int2.var);
                graph.entry(int2.var).or_insert_with(HashSet::new).insert(int1.var);
            }
        }
    }
    
    graph
}

/// Compute which variables are live across function calls
/// These variables cannot be allocated to caller-saved registers
fn compute_live_across_call(intervals: &[LiveInterval], func: &IrFunction) -> HashSet<VarId> {
    let mut live_across_call = HashSet::new();
    let mut position = 0;
    
    // Find all call instruction positions.
    // IMPORTANT: must use the SAME position numbering as compute_live_intervals,
    // which increments position by 1 for terminators in addition to instructions.
    let mut call_positions = Vec::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if matches!(inst, IrInstruction::Call { .. } | IrInstruction::IndirectCall { .. }) {
                call_positions.push(position);
            }
            position += 1;
        }
        position += 1; // account for terminator (matching compute_live_intervals)
    }
    
    // Mark variables whose live ranges span any call position
    for interval in intervals {
        for &call_pos in &call_positions {
            if interval.start < call_pos && call_pos < interval.end {
                live_across_call.insert(interval.var);
                break;
            }
        }
    }
    
    live_across_call
}

fn color_graph(intervals: &mut [LiveInterval], interference: &HashMap<VarId, HashSet<VarId>>, copy_hints: &HashMap<VarId, VarId>, param_hints: &HashMap<VarId, PhysicalReg>, use_callee_saved: bool, live_across_call: &HashSet<VarId>, target: &model::TargetConfig) {
    // Sort by spill cost heuristic (shorter intervals = higher priority)
    intervals.sort_by_key(|i| i.end - i.start);
    
    let available_regs = PhysicalReg::allocatable(target);
    
    // Build a map of var -> register for already colored intervals
    let mut var_colors: HashMap<VarId, PhysicalReg> = HashMap::new();
    
    // Greedy coloring with copy coalescing and preference for caller-saved registers
    for i in 0..intervals.len() {
        let mut used_colors: HashSet<PhysicalReg> = HashSet::new();
        let current_var = intervals[i].var;
        
        // Check which colors are used by interfering neighbors
        if let Some(neighbors) = interference.get(&current_var) {
            for neighbor in neighbors {
                if let Some(reg) = var_colors.get(neighbor) {
                    used_colors.insert(*reg);
                }
            }
        }

        // Determine early whether this variable must survive function calls.
        // Such variables cannot be in caller-saved registers.
        let is_live_across_call = live_across_call.contains(&current_var);
        let callee_saved_set: HashSet<PhysicalReg> = PhysicalReg::callee_saved(target).into_iter().collect();
        
        // Try parameter hint first (prefer incoming parameter registers)
        // But only if the hinted register is safe (callee-saved, or var is not live-across-call)
        let mut assigned_reg: Option<PhysicalReg> = None;
        if let Some(&hint_reg) = param_hints.get(&current_var) {
            let reg_is_safe = !is_live_across_call || callee_saved_set.contains(&hint_reg);
            if reg_is_safe && !used_colors.contains(&hint_reg) && available_regs.contains(&hint_reg) {
                assigned_reg = Some(hint_reg);
            }
        }
        
        // Try to coalesce with copy hint if param hint didn't work
        // Same safety restriction: don't coalesce into caller-saved if live-across-call
        if assigned_reg.is_none() {
            if let Some(hint_var) = copy_hints.get(&current_var) {
                if let Some(hint_reg) = var_colors.get(hint_var) {
                    let interferes = interference.get(&current_var)
                        .map(|neighbors| neighbors.contains(hint_var))
                        .unwrap_or(false);
                    let reg_is_safe = !is_live_across_call || callee_saved_set.contains(hint_reg);
                    if !interferes && reg_is_safe && !used_colors.contains(hint_reg) && available_regs.contains(hint_reg) {
                        assigned_reg = Some(*hint_reg);
                    }
                }
            }
        }
        
        // If coalescing didn't work, try to assign a register with caller-saved preference
        // BUT skip caller-saved if this variable is live across a call
        if assigned_reg.is_none() && !is_live_across_call {
            for reg in PhysicalReg::caller_saved(target) {
                if !used_colors.contains(&reg) && available_regs.contains(&reg) {
                    assigned_reg = Some(reg);
                    break;
                }
            }
        }
        
        // Fall back to callee-saved if caller-saved are exhausted/forbidden (and allowed).
        // Also forced when the variable is live across a call, regardless of the heuristic,
        // because we *must* preserve it — using a caller-saved register would be wrong.
        if assigned_reg.is_none() && (use_callee_saved || is_live_across_call) {
            for reg in PhysicalReg::callee_saved(target) {
                if !used_colors.contains(&reg) && available_regs.contains(&reg) {
                    assigned_reg = Some(reg);
                    break;
                }
            }
        }
        
        // If live across call and no callee-saved available, do NOT fall back to a
        // caller-saved register (it would be clobbered). Leave allocated register as None;
        // var_to_op will assign a stack slot which survives calls.
        // (This is conservative register spilling: prefer correctness over register pressure.)
        
        // Update interval and tracking map
        intervals[i].reg = assigned_reg;
        if let Some(reg) = assigned_reg {
            var_colors.insert(current_var, reg);
        }
    }
}
