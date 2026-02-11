use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, Instruction, Function, Operand, Terminator};
use model::Type;

/// Mem2reg optimization pass: promotes memory allocations to SSA registers
/// 
/// This pass identifies stack-allocated variables (allocas) that can be promoted
/// to virtual registers in SSA form, eliminating unnecessary Load/Store instructions.
/// 
/// A variable can be promoted if:
/// 1. It's a scalar type (not an array or struct)
/// 2. Its address is never taken (except for Load/Store operations)
/// 3. All uses are Load/Store operations
pub fn mem2reg(func: &mut Function) {
    // Step 1: Identify promotable allocas
    let promotable = identify_promotable_allocas(func);
    
    if promotable.is_empty() {
        return; // No work to do
    }
    
    // Step 2: For each promotable alloca, track what value is stored per block
    let mut var_values: HashMap<VarId, HashMap<BlockId, VarId>> = HashMap::new();
    
    // Initialize with undef for each promotable alloca
    for alloca_id in &promotable {
        var_values.insert(*alloca_id, HashMap::new());
    }
    
    // Step 3: Process blocks in dominance order and replace Load/Store
    // For now, use a simple forward pass (works for most cases)
    for block_idx in 0..func.blocks.len() {
        let block_id = func.blocks[block_idx].id;
        let mut new_instructions = Vec::new();
        let mut current_block_values: HashMap<VarId, VarId> = HashMap::new();
        
        // Copy inherited values from this block's tracking
        for (alloca_id, block_map) in &var_values {
            if let Some(val) = block_map.get(&block_id) {
                current_block_values.insert(*alloca_id, *val);
            }
        }
        
        for instr in &func.blocks[block_idx].instructions {
            match instr {
                Instruction::Alloca { dest, .. } if promotable.contains(dest) => {
                    // Skip - remove alloca completely
                    continue;
                }
                Instruction::Store { addr: Operand::Var(alloca_id), src: Operand::Var(src_var), .. } 
                    if promotable.contains(alloca_id) => {
                    // Track the stored value for this alloca in this block
                    current_block_values.insert(*alloca_id, *src_var);
                    var_values.get_mut(alloca_id).unwrap().insert(block_id, *src_var);
                    // Skip - remove store
                    continue;
                }
                Instruction::Store { addr: Operand::Var(alloca_id), src: Operand::Constant(c), .. } 
                    if promotable.contains(alloca_id) => {
                    // Need to create a Copy instruction for the constant 
                    // But we can't know the dest var here... skip for now
                    // TODO: Handle constant stores properly
                    new_instructions.push(instr.clone());
                }
                Instruction::Load { dest, addr: Operand::Var(alloca_id), .. } 
                    if promotable.contains(alloca_id) => {
                    // Replace load with value from current block
                    if let Some(src_var) = current_block_values.get(alloca_id) {
                        new_instructions.push(Instruction::Copy {
                            dest: *dest,
                            src: Operand::Var(*src_var),
                        });
                    } else {
                        // No value stored yet - this is an undefined behavior in C
                        // but we'll keep the load to avoid crashing
                        new_instructions.push(instr.clone());
                    }
                    continue;
                }
                _ => {
                    new_instructions.push(instr.clone());
                }
            }
        }
        
        func.blocks[block_idx].instructions = new_instructions;
        
        // Propagate values to successor blocks
        match &func.blocks[block_idx].terminator {
            Terminator::Br(target) => {
                for (alloca_id, val) in &current_block_values {
                    var_values.get_mut(alloca_id).unwrap()
                        .entry(*target)
                        .or_insert(*val);
                }
            }
            Terminator::CondBr { then_block, else_block, .. } => {
                for (alloca_id, val) in &current_block_values {
                    var_values.get_mut(alloca_id).unwrap()
                        .entry(*then_block)
                        .or_insert(*val);
                    var_values.get_mut(alloca_id).unwrap()
                        .entry(*else_block)
                        .or_insert(*val);
                }
            }
            _ => {}
        }
    }
}

/// Identify which allocas can be promoted to registers
fn identify_promotable_allocas(func: &Function) -> HashSet<VarId> {
    let mut promotable = HashSet::new();
    let mut alloca_types: HashMap<VarId, Type> = HashMap::new();
    
    // Collect all allocas with their types
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Instruction::Alloca { dest, r#type } = instr {
                alloca_types.insert(*dest, r#type.clone());
            }
        }
    }
    
    // Check which allocas are promotable
    for (alloca_id, alloca_type) in &alloca_types {
        // Only promote scalar types (not arrays or structs)
        if !is_scalar_type(alloca_type) {
            continue;
        }
        
        // Check if address is taken (used in ways other than Load/Store)
        if is_address_taken(func, *alloca_id) {
            continue;
        }
        
        promotable.insert(*alloca_id);
    }
    
    promotable
}

/// Check if a type is a scalar (promotable) type
fn is_scalar_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int | Type::UnsignedInt | Type::Char | Type::UnsignedChar |
        Type::Short | Type::UnsignedShort | Type::Long | Type::UnsignedLong |
        Type::LongLong | Type::UnsignedLongLong | Type::Float | Type::Double |
        Type::Pointer(_)
    )
}

/// Check if an alloca's address is taken (used for anything other than Load/Store)
fn is_address_taken(func: &Function, alloca_id: VarId) -> bool {
    for block in &func.blocks {
        for instr in &block.instructions {
            match instr {
                // These are OK - they use the address but don't "take" it
                Instruction::Load { addr: Operand::Var(id), .. } if *id == alloca_id => continue,
                Instruction::Store { addr: Operand::Var(id), .. } if *id == alloca_id => continue,
                
                // Any other use of this variable means its address is taken
                _ => {
                    if instruction_uses_var(instr, alloca_id) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if an instruction uses a specific variable
fn instruction_uses_var(instr: &Instruction, var_id: VarId) -> bool {
    match instr {
        Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => {
            operand_uses_var(left, var_id) || operand_uses_var(right, var_id)
        }
        Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => {
            operand_uses_var(src, var_id)
        }
        Instruction::Copy { src, .. } => operand_uses_var(src, var_id),
        Instruction::Load { addr, .. } => operand_uses_var(addr, var_id),
        Instruction::Store { addr, src, .. } => {
            operand_uses_var(addr, var_id) || operand_uses_var(src, var_id)
        }
        Instruction::GetElementPtr { base, index, .. } => {
            operand_uses_var(base, var_id) || operand_uses_var(index, var_id)
        }
        Instruction::Call { args, .. } | Instruction::IndirectCall { args, .. } => {
            args.iter().any(|arg| operand_uses_var(arg, var_id))
        }
        Instruction::InlineAsm { inputs, .. } => {
            inputs.iter().any(|input| operand_uses_var(input, var_id))
        }
        Instruction::Phi { .. } | Instruction::Alloca { .. } => false,
    }
}

/// Check if an operand uses a specific variable
fn operand_uses_var(operand: &Operand, var_id: VarId) -> bool {
    matches!(operand, Operand::Var(id) if *id == var_id)
}
