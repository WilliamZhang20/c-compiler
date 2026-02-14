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
    
    // Caller-saved registers (volatile in Windows x64)
    pub fn caller_saved() -> Vec<PhysicalReg> {
        vec![Self::Rax, Self::Rcx, Self::Rdx, Self::R8, Self::R9, Self::R10, Self::R11]
    }
    
    // Callee-saved registers (non-volatile in Windows x64)
    pub fn callee_saved() -> Vec<PhysicalReg> {
        vec![Self::Rbx, Self::Rsi, Self::Rdi, Self::R12, Self::R13, Self::R14, Self::R15]
    }
    
    // All allocatable registers
    // Exclude Rax, Rcx, Rdx as they are used as scratch registers in codegen
    // Exclude R10, R11 as they are used as scratch for address loading
    pub fn allocatable() -> Vec<PhysicalReg> {
        vec![
            Self::Rbx, Self::Rsi, Self::Rdi, 
            Self::R8, Self::R9, 
            Self::R12, Self::R13, Self::R14, Self::R15,
        ]
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

/// allocate_registers performs graph-coloring register allocation
pub fn allocate_registers(func: &IrFunction) -> HashMap<VarId, PhysicalReg> {
    // 1. Compute live intervals for each variable
    let mut intervals = compute_live_intervals(func);
    
    // Sort intervals by var ID to be deterministic
    intervals.sort_by_key(|i| i.var);
    
    // 2. Build interference graph
    let interference = build_interference_graph(&intervals);
    
    // 3. Graph coloring with prioritization heuristics
    color_graph(&mut intervals, &interference);
    
    // 4. Build result map
    let mut reg_alloc = HashMap::new();
    for interval in &intervals {
        if let Some(reg) = interval.reg {
            reg_alloc.insert(interval.var, reg);
        }
    }
    
    reg_alloc
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

fn color_graph(intervals: &mut [LiveInterval], interference: &HashMap<VarId, HashSet<VarId>>) {
    // Sort by spill cost heuristic (shorter intervals = higher priority)
    intervals.sort_by_key(|i| i.end - i.start);
    
    let available_regs = PhysicalReg::allocatable();
    
    // Build a map of var -> register for already colored intervals
    let mut var_colors: HashMap<VarId, PhysicalReg> = HashMap::new();
    
    // Greedy coloring with preference for caller-saved registers (no save/restore overhead)
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
        
        // Try to assign a register, preferring caller-saved (no save/restore needed)
        let mut assigned_reg: Option<PhysicalReg> = None;
        for reg in PhysicalReg::caller_saved() {
            if !used_colors.contains(&reg) && available_regs.contains(&reg) {
                assigned_reg = Some(reg);
                break;
            }
        }
        
        // Fall back to callee-saved if caller-saved are exhausted
        if assigned_reg.is_none() {
            for reg in PhysicalReg::callee_saved() {
                if !used_colors.contains(&reg) && available_regs.contains(&reg) {
                    assigned_reg = Some(reg);
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
