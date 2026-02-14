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

    /// Get all predecessor blocks for a given block (with caching)
    pub(crate) fn get_predecessors(&mut self, block: BlockId) -> Vec<BlockId> {
        // Check cache first
        if self.pred_cache_valid {
            if let Some(preds) = self.pred_cache.get(&block) {
                return preds.clone();
            }
        }
        
        // Compute predecessors
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
        
        // Update cache if it's valid
        if self.pred_cache_valid {
            self.pred_cache.insert(block, preds.clone());
        }
        
        preds
    }
    
    /// Invalidate the predecessor cache (call when CFG changes)
    #[allow(dead_code)]
    pub(crate) fn invalidate_pred_cache(&mut self) {
        self.pred_cache_valid = false;
        self.pred_cache.clear();
    }
    
    /// Rebuild the predecessor cache for all blocks
    #[allow(dead_code)]
    pub(crate) fn rebuild_pred_cache(&mut self) {
        self.pred_cache.clear();
        
        // Initialize empty predecessor lists for all blocks
        for block in &self.blocks {
            self.pred_cache.insert(block.id, Vec::new());
        }
        
        // Populate predecessor lists
        for block in &self.blocks {
            match &block.terminator {
                Terminator::Br(target) => {
                    self.pred_cache.entry(*target).or_default().push(block.id);
                }
                Terminator::CondBr { then_block, else_block, .. } => {
                    self.pred_cache.entry(*then_block).or_default().push(block.id);
                    self.pred_cache.entry(*else_block).or_default().push(block.id);
                }
                _ => {}
            }
        }
        
        self.pred_cache_valid = true;
    }
}
