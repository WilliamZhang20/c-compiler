use std::collections::HashMap;
use crate::types::{VarId, BlockId, Instruction, Terminator};
use crate::lowerer::Lowerer;

/// SSA construction trait for the Lowerer
impl Lowerer {
    /// Seal a block after all predecessors are known
    pub(crate) fn seal_block(&mut self, block: BlockId) {
        if self.sealed_blocks.contains(&block) { 
            return; 
        }
        let phis = self.incomplete_phis.remove(&block).unwrap_or_default();
        for (name, phi_var) in phis {
            self.add_phi_operands(&name, block, phi_var);
        }
        self.sealed_blocks.insert(block);
    }

    /// Write a variable definition at a specific block
    pub(crate) fn write_variable(&mut self, name: &str, block: BlockId, value: VarId) {
        self.variable_defs.entry(name.to_string())
            .or_insert_with(HashMap::new)
            .insert(block, value);
    }

    /// Read a variable at a specific block, creating Phi nodes as needed
    pub(crate) fn read_variable(&mut self, name: &str, block: BlockId) -> VarId {
        if let Some(defs) = self.variable_defs.get(name) {
            if let Some(var) = defs.get(&block) {
                return *var;
            }
        }
        self.read_variable_recursive(name, block)
    }

    /// Recursively read variable, handling Phi node creation for non-sealed blocks
    pub(crate) fn read_variable_recursive(&mut self, name: &str, block: BlockId) -> VarId {
        let mut val;
        if !self.sealed_blocks.contains(&block) {
            // Incomplete Phi
            val = self.new_var();
            self.incomplete_phis.entry(block)
                .or_insert_with(HashMap::new)
                .insert(name.to_string(), val);
        } else {
            let preds = self.get_predecessors(block);
            if preds.len() == 1 {
                val = self.read_variable(name, preds[0]);
            } else {
                val = self.new_var();
                self.write_variable(name, block, val);
                val = self.add_phi_operands(name, block, val);
            }
        }
        self.write_variable(name, block, val);
        val
    }

    /// Add Phi operands for all predecessors of a block
    pub(crate) fn add_phi_operands(&mut self, name: &str, block: BlockId, phi_var: VarId) -> VarId {
        let preds = self.get_predecessors(block);
        let mut phi_preds = Vec::new();
        for pred in preds {
            let val = self.read_variable(name, pred);
            phi_preds.push((pred, val));
        }
        // Actually insert the Phi instruction at the beginning of the block
        self.blocks[block.0].instructions.insert(0, Instruction::Phi {
            dest: phi_var,
            preds: phi_preds,
        });
        // Trivial phi elimination could go here
        phi_var
    }

    /// Get all predecessor blocks for a given block
    pub(crate) fn get_predecessors(&self, block: BlockId) -> Vec<BlockId> {
        let mut preds = Vec::new();
        for b in &self.blocks {
            match &b.terminator {
                Terminator::Br(id) if *id == block => preds.push(b.id),
                Terminator::CondBr { then_block, else_block, .. } => {
                    if *then_block == block { preds.push(b.id); }
                    if *else_block == block { preds.push(b.id); }
                }
                _ => {}
            }
        }
        preds
    }
}
