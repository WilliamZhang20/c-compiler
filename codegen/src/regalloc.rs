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
    let mut intervals: HashMap<VarId, (usize, usize)> = HashMap::new();
    let mut alloca_vars: HashSet<VarId> = HashSet::new();
    let mut position = 0;
    
    // First pass: identify alloca variables (pointers that shouldn't be in registers)
    for block in &func.blocks {
        for inst in &block.instructions {
            if let IrInstruction::Alloca { dest, .. } = inst {
                alloca_vars.insert(*dest);
            }
        }
    }
    
    // Assign positions to all instructions
    for block in &func.blocks {
        for inst in &block.instructions {
            // Record defs (skip alloca vars)
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
                    // InlineAsm can define multiple outputs, handle first one for now
                    outputs.first().copied()
                }
                IrInstruction::VaArg { dest, .. } => Some(*dest),
                IrInstruction::Alloca { .. } | IrInstruction::Store { .. } 
                | IrInstruction::VaStart { .. } | IrInstruction::VaEnd { .. } | IrInstruction::VaCopy { .. } => None,
            };
            
            if let Some(var) = def_var {
                if !alloca_vars.contains(&var) {
                    intervals.entry(var).or_insert((position, position)).1 = position;
                }
            }
            
            // Record uses (skip alloca vars)
            visit_operands(inst, |var| {
                if !alloca_vars.contains(&var) {
                    intervals.entry(var).or_insert((position, position)).1 = position;
                }
            });
            
            position += 1;
        }
        
        // Handle terminator
        if let IrTerminator::CondBr { cond, .. } = &block.terminator {
            if let Operand::Var(v) = cond {
                if !alloca_vars.contains(v) {
                    intervals.entry(*v).or_insert((position, position)).1 = position;
                }
            }
        } else if let IrTerminator::Ret(Some(Operand::Var(v))) = &block.terminator {
            if !alloca_vars.contains(v) {
                intervals.entry(*v).or_insert((position, position)).1 = position;
            }
        }
        position += 1;
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
    
    // Find all call instruction positions
    let mut call_positions = Vec::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if matches!(inst, IrInstruction::Call { .. } | IrInstruction::IndirectCall { .. }) {
                call_positions.push(position);
            }
            position += 1;
        }
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
        
        // Try parameter hint first (prefer incoming parameter registers)
        let mut assigned_reg: Option<PhysicalReg> = None;
        if let Some(&hint_reg) = param_hints.get(&current_var) {
            // Check if we can use the hinted register (not used by neighbors and available)
            if !used_colors.contains(&hint_reg) && available_regs.contains(&hint_reg) {
                assigned_reg = Some(hint_reg);
            }
        }
        
        // Try to coalesce with copy hint if param hint didn't work
        if assigned_reg.is_none() {
            if let Some(hint_var) = copy_hints.get(&current_var) {
                if let Some(hint_reg) = var_colors.get(hint_var) {
                    // Check if we can use the same register (not interfering and not already used)
                    let interferes = interference.get(&current_var)
                        .map(|neighbors| neighbors.contains(hint_var))
                        .unwrap_or(false);
                    
                    if !interferes && !used_colors.contains(hint_reg) && available_regs.contains(hint_reg) {
                        assigned_reg = Some(*hint_reg);
                    }
                }
            }
        }
        
        // Determine which registers are available for this variable
        // Variables live across calls cannot use caller-saved registers
        let is_live_across_call = live_across_call.contains(&current_var);
        
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
        
        // If live across call and no callee-saved available, must use ANY non-interfering register
        // (including caller-saved as last resort, will require spill/restore)
        if assigned_reg.is_none() && is_live_across_call {
            for reg in &available_regs {
                if !used_colors.contains(reg) {
                    assigned_reg = Some(*reg);
                    break;
                }
            }
        }
        
        // Update interval and tracking map
        intervals[i].reg = assigned_reg;
        if let Some(reg) = assigned_reg {
            var_colors.insert(current_var, reg);
        }
    }
}
