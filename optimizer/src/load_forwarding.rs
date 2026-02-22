use ir::{Function, Instruction, Operand};
use model::Type;
use std::collections::HashMap;

/// Load forwarding: eliminate redundant loads within a basic block
///
/// When a value is stored to an address and later loaded from the same address
/// with no intervening aliasing store, replace the load with a copy of the
/// stored value. This eliminates unnecessary memory traffic.
///
/// This is a conservative intra-block analysis:
/// - Tracks store addr → stored value mappings
/// - A store to an address records that the address holds a known value
/// - A load from a known address is replaced by a copy from the stored value
/// - Any call or indirect call invalidates all known addresses (may alias anything)
/// - A store to a different address does NOT invalidate others (SSA addresses are distinct)
pub fn load_forwarding(func: &mut Function) {
    for block in &mut func.blocks {
        // Map from address operand → (stored value operand, value_type)
        let mut known_stores: HashMap<Operand, (Operand, Type)> = HashMap::new();
        let mut replacements: Vec<(usize, Instruction)> = Vec::new();

        for (i, inst) in block.instructions.iter().enumerate() {
            match inst {
                Instruction::Store { addr, src, value_type } => {
                    // Record that this address now holds this value
                    known_stores.insert(addr.clone(), (src.clone(), value_type.clone()));
                }
                Instruction::Load { dest, addr, value_type } => {
                    if let Some((stored_val, stored_type)) = known_stores.get(addr) {
                        // Types must match for the forwarding to be correct
                        if stored_type == value_type {
                            // Replace load with copy from the stored value
                            replacements.push((i, Instruction::Copy {
                                dest: *dest,
                                src: stored_val.clone(),
                            }));
                        }
                    }
                    // A load doesn't invalidate anything (read-only)
                }
                // Calls may write to any memory — invalidate everything
                Instruction::Call { .. } | Instruction::IndirectCall { .. } => {
                    known_stores.clear();
                }
                // InlineAsm may also have side effects
                Instruction::InlineAsm { .. } => {
                    known_stores.clear();
                }
                // VaStart/VaEnd/VaCopy/VaArg modify va_list memory
                Instruction::VaStart { .. } | Instruction::VaEnd { .. }
                | Instruction::VaCopy { .. } | Instruction::VaArg { .. } => {
                    known_stores.clear();
                }
                // Other instructions (Binary, Unary, Copy, Cast, Alloca, GEP)
                // don't write to memory, so they don't invalidate stores
                _ => {}
            }
        }

        // Apply replacements
        for (idx, new_inst) in replacements {
            block.instructions[idx] = new_inst;
        }
    }

    // Dead store elimination: remove stores that are overwritten before being read
    dead_store_elimination(func);
}

/// Dead store elimination within a basic block.
///
/// Scans instructions in reverse order. If we see a store to an address
/// that is already in the "will be overwritten" set (meaning a later store
/// to the same address exists with no intervening load/call), the earlier
/// store is dead and can be removed.
fn dead_store_elimination(func: &mut Function) {
    use std::collections::HashSet;

    for block in &mut func.blocks {
        // Scan backwards to find dead stores
        // Track addresses that are stored to without being loaded
        let mut overwritten: HashSet<Operand> = HashSet::new();
        let mut dead_indices: Vec<usize> = Vec::new();

        for i in (0..block.instructions.len()).rev() {
            match &block.instructions[i] {
                Instruction::Store { addr, .. } => {
                    if overwritten.contains(addr) {
                        // This store is dead — a later store overwrites it
                        dead_indices.push(i);
                    } else {
                        overwritten.insert(addr.clone());
                    }
                }
                Instruction::Load { addr, .. } => {
                    // A load reads from this address — earlier stores are NOT dead
                    overwritten.remove(addr);
                }
                // Calls/asm may read any memory — clear all
                Instruction::Call { .. } | Instruction::IndirectCall { .. }
                | Instruction::InlineAsm { .. }
                | Instruction::VaStart { .. } | Instruction::VaEnd { .. }
                | Instruction::VaCopy { .. } | Instruction::VaArg { .. } => {
                    overwritten.clear();
                }
                _ => {}
            }
        }

        // Remove dead stores (in reverse order to maintain indices)
        dead_indices.sort();
        for &idx in dead_indices.iter().rev() {
            block.instructions.remove(idx);
        }
    }
}
