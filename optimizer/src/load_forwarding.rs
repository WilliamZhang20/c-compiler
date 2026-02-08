// Load forwarding optimization: eliminate redundant loads from the same memory location
//
// If we have:
//   %1 = load [addr]
//   ... (no stores to addr)
//   %2 = load [addr]
//
// We can replace all uses of %2 with %1, eliminating the redundant load.
//
// This is particularly effective for loop-invariant loads and repeated array accesses.

use ir::{BasicBlock, Function, Instruction, Operand, Terminator, VarId};
use std::collections::HashMap;

/// Performs load forwarding within a function
///
/// Scans each basic block for redundant loads and forwards earlier load results.
/// Works within basic blocks to avoid complexity of cross-block analysis.
pub fn load_forwarding(func: &mut Function) {
    for block in &mut func.blocks {
        forward_loads_in_block(block);
    }
}

fn forward_loads_in_block(block: &mut BasicBlock) {
    // Map from load address operand to the VarId that holds the loaded value
    let mut load_map: HashMap<String, VarId> = HashMap::new();
    let mut replacements: HashMap<VarId, VarId> = HashMap::new();
    
    for inst in &block.instructions {
        match inst {
            Instruction::Load { dest, addr, .. } => {
                let addr_key = operand_key(addr);
                if let Some(prev_dest) = load_map.get(&addr_key) {
                    // Found redundant load! Forward the previous load result
                    replacements.insert(*dest, *prev_dest);
                } else {
                    // First load from this address
                    load_map.insert(addr_key, *dest);
                }
            }
            Instruction::Store { addr, .. } => {
                // Store invalidates loads from this address
                let addr_key = operand_key(addr);
                load_map.remove(&addr_key);
                // Conservative: also clear all loads if addr involves variables
                // (could be aliasing)
                if matches!(addr, Operand::Var(_)) {
                    load_map.clear();
                }
            }
            Instruction::Call { .. } | Instruction::IndirectCall { .. } => {
                // Function calls may have side effects, clear all loads
                load_map.clear();
            }
            // Any instruction that writes to a variable could invalidate loads
            // if that variable was used as an address
            Instruction::Binary { dest, .. }
            | Instruction::FloatBinary { dest, .. }
            | Instruction::Unary { dest, .. }
            | Instruction::FloatUnary { dest, .. }
            | Instruction::Copy { dest, .. }
            | Instruction::GetElementPtr { dest, .. }
            | Instruction::Alloca { dest, .. } => {
                // If any load used this dest variable as an address, invalidate it
                load_map.retain(|key, _| !key.contains(&format!("var_{}", dest.0)));
            }
            _ => {}
        }
    }
    
    // Apply replacements
    if !replacements.is_empty() {
        for inst in &mut block.instructions {
            replace_operands_in_instruction(inst, &replacements);
        }
        
        // Also replace in terminator
        replace_operands_in_terminator(&mut block.terminator, &replacements);
    }
}

fn operand_key(operand: &Operand) -> String {
    match operand {
        Operand::Var(v) => format!("var_{}", v.0),
        Operand::Global(name) => format!("global_{}", name),
        Operand::Constant(c) => format!("const_{}", c),
        Operand::FloatConstant(f) => format!("float_{}", f),
    }
}

fn replace_operands_in_instruction(inst: &mut Instruction, replacements: &HashMap<VarId, VarId>) {
    match inst {
        Instruction::Binary { left, right, .. } => {
            replace_operand(left, replacements);
            replace_operand(right, replacements);
        }
        Instruction::FloatBinary { left, right, .. } => {
            replace_operand(left, replacements);
            replace_operand(right, replacements);
        }
        Instruction::Unary { src, .. } => {
            replace_operand(src, replacements);
        }
        Instruction::FloatUnary { src, .. } => {
            replace_operand(src, replacements);
        }
        Instruction::Copy { src, .. } => {
            replace_operand(src, replacements);
        }
        Instruction::Load { addr, .. } => {
            replace_operand(addr, replacements);
        }
        Instruction::Store { addr, src, .. } => {
            replace_operand(addr, replacements);
            replace_operand(src, replacements);
        }
        Instruction::Call { args, .. } => {
            for arg in args {
                replace_operand(arg, replacements);
            }
        }
        Instruction::IndirectCall { func_ptr, args, .. } => {
            replace_operand(func_ptr, replacements);
            for arg in args {
                replace_operand(arg, replacements);
            }
        }
        Instruction::GetElementPtr { base, index, .. } => {
            replace_operand(base, replacements);
            replace_operand(index, replacements);
        }
        _ => {}
    }
}

fn replace_operands_in_terminator(term: &mut Terminator, replacements: &HashMap<VarId, VarId>) {
    match term {
        Terminator::Ret(Some(operand)) => {
            replace_operand(operand, replacements);
        }
        Terminator::CondBr { cond, .. } => {
            replace_operand(cond, replacements);
        }
        _ => {}
    }
}

fn replace_operand(operand: &mut Operand, replacements: &HashMap<VarId, VarId>) {
    if let Operand::Var(v) = operand {
        if let Some(new_var) = replacements.get(v) {
            *v = *new_var;
        }
    }
}
