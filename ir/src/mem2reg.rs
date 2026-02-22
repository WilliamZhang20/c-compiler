use std::collections::{HashMap, HashSet};
use crate::types::{VarId, BlockId, Instruction, Function, Operand, Terminator};
use model::Type;

/// Mem2reg optimization pass: promotes memory allocations to SSA registers
pub fn mem2reg(func: &mut Function) {
    let mut pass = Mem2RegPass::new(func);
    pass.run();
    // Verify SSA invariants after promotion.  Catches undefined-VarId bugs
    // (like the transitive-simplified-phi issue) before they become runtime
    // segfaults downstream in codegen/regalloc.
    debug_assert!(crate::ssa_utils::verify_ssa(func).is_ok(), "SSA verification failed after mem2reg for function '{}': {}",
        func.name, crate::ssa_utils::verify_ssa(func).unwrap_err());
}

struct Mem2RegPass<'a> {
    func: &'a mut Function,
    preds: HashMap<BlockId, Vec<BlockId>>,
    promotable: HashSet<VarId>,
    // Last definition of a var in a block
    block_defs: HashMap<VarId, HashMap<BlockId, VarId>>,
    // Cache for incoming values to blocks
    incoming_cache: HashMap<(BlockId, VarId), VarId>,
    // New instructions collection
    new_insts: HashMap<BlockId, Vec<Instruction>>,
    // Phi instructions to add to block headers
    block_phis: HashMap<BlockId, Vec<Instruction>>,
    
    next_var_id: usize,
    alloca_types: HashMap<VarId, Type>,
    
    // Shared zero constants for uninitialized values
    zero_int: Option<VarId>,
    zero_float: Option<VarId>,
    // Track phi_vars that were simplified away (phi_var -> replacement)
    simplified: HashMap<VarId, VarId>,
}

impl<'a> Mem2RegPass<'a> {
    fn new(func: &'a mut Function) -> Self {
        let next_var_id = Self::find_max_var_id(func) + 1;
        Self {
            func,
            preds: HashMap::new(),
            promotable: HashSet::new(),
            block_defs: HashMap::new(),
            incoming_cache: HashMap::new(),
            new_insts: HashMap::new(),
            block_phis: HashMap::new(),
            next_var_id,
            alloca_types: HashMap::new(),
            zero_int: None,
            zero_float: None,
            simplified: HashMap::new(),
        }
    }

    fn run(&mut self) {
        // println!("Running mem2reg on function {}", self.func.name);
        self.compute_preds();
        self.identify_promotable_allocas();
        
        if self.promotable.is_empty() { return; }
        
        // Create zero constants in entry block if needed
        self.ensure_zeros();
        
        // Pre-pass: Canonicalize stores and collect block defs
        self.prepare_defs();
        
        // Pass 2: Rewrite
        let block_indices: Vec<usize> = (0..self.func.blocks.len()).collect();
        for i in block_indices {
             self.process_block(i);
        }
        
        self.reconstruct_blocks();
        self.fixup_simplified_phi_sources();
    }
    
    fn compute_preds(&mut self) {
        // Initialize all blocks in map
        for block in &self.func.blocks {
            self.preds.insert(block.id, Vec::new());
        }
        // Populate preds
        for block in &self.func.blocks {
            match &block.terminator {
                Terminator::Br(target) => {
                    self.preds.entry(*target).or_default().push(block.id);
                }
                Terminator::CondBr { then_block, else_block, .. } => {
                    self.preds.entry(*then_block).or_default().push(block.id);
                    self.preds.entry(*else_block).or_default().push(block.id);
                }
                _ => {}
            }
        }
    }

    fn identify_promotable_allocas(&mut self) {
        let mut alloca_types = HashMap::new();
        for block in &self.func.blocks {
            for instr in &block.instructions {
                if let Instruction::Alloca { dest, r#type } = instr {
                    alloca_types.insert(*dest, r#type.clone());
                }
            }
        }
        
        for (id, ty) in &alloca_types {
            if Self::is_scalar_type(ty) && !Self::is_address_taken(self.func, *id) {
                self.promotable.insert(*id);
            }
        }
        self.alloca_types = alloca_types;
    }
    
    fn ensure_zeros(&mut self) {
        let z_int = self.new_var();
        let z_float = self.new_var();
        self.zero_int = Some(z_int);
        self.zero_float = Some(z_float);
        
        // Insert definitions at start of entry block
        if let Some(entry) = self.func.blocks.first_mut() {
            entry.instructions.insert(0, Instruction::Copy { dest: z_float, src: Operand::FloatConstant(0.0) });
            entry.instructions.insert(0, Instruction::Copy { dest: z_int, src: Operand::Constant(0) });
        }
    }

    fn prepare_defs(&mut self) {
        let blocks = &mut self.func.blocks;
        let next_var_id = &mut self.next_var_id;
        let promotable = &self.promotable;
        let block_defs = &mut self.block_defs;

        for block in blocks {
            let mut i = 0;
            while i < block.instructions.len() {
                // If Store(alloca, Constant), convert to Copy + Store(alloca, Var)
                let mut inject_copy = None;
                if let Instruction::Store { addr: Operand::Var(alloca_id), src, .. } = &block.instructions[i] {
                    if promotable.contains(alloca_id) && !matches!(src, Operand::Var(_)) {
                        let c = src.clone();
                        inject_copy = Some(c);
                    }
                }
                
                if let Some(c) = inject_copy {
                    let new_v = VarId(*next_var_id);
                    *next_var_id += 1;
                    
                    // Insert Copy
                    block.instructions.insert(i, Instruction::Copy { dest: new_v, src: c });
                    // Update Store to use new_v
                    if let Instruction::Store { src, .. } = &mut block.instructions[i+1] {
                        *src = Operand::Var(new_v);
                    }
                    i += 1; // Advance past Copy
                }
                
                // Now verify Store has a Var source
                if let Instruction::Store { addr: Operand::Var(alloca_id), src: Operand::Var(src_var), .. } = &block.instructions[i] {
                   if promotable.contains(alloca_id) {
                        block_defs.entry(*alloca_id).or_default().insert(block.id, *src_var);
                   }
                }
                i += 1;
            }
        }
    }

    fn process_block(&mut self, block_idx: usize) {
        let block_id = self.func.blocks[block_idx].id;
        let mut new_instructions = Vec::new();
        let mut local_defs: HashMap<VarId, VarId> = HashMap::new(); 
        
        // Temporarily take instructions to avoid cloning while allowing mutable self access
        let instructions = std::mem::take(&mut self.func.blocks[block_idx].instructions);
        
        for instr in &instructions {
            match instr {
                Instruction::Alloca { dest, .. } if self.promotable.contains(dest) => {
                    // Remove
                }
                Instruction::Store { addr: Operand::Var(alloca_id), src: Operand::Var(src_var), .. } 
                    if self.promotable.contains(alloca_id) => {
                    local_defs.insert(*alloca_id, *src_var);
                    // Remove Store
                }
                Instruction::Load { dest, addr: Operand::Var(alloca_id), .. }
                    if self.promotable.contains(alloca_id) => {
                    let val_var = if let Some(v) = local_defs.get(alloca_id) {
                        *v
                    } else {
                        self.get_incoming_value(block_id, *alloca_id)
                    };
                    new_instructions.push(Instruction::Copy { dest: *dest, src: Operand::Var(val_var) });
                }
                _ => {
                    new_instructions.push(instr.clone());
                }
            }
        }
        self.new_insts.insert(block_id, new_instructions);
    }

    fn get_incoming_value(&mut self, block_id: BlockId, var_id: VarId) -> VarId {
        if let Some(val) = self.incoming_cache.get(&(block_id, var_id)) {
            return *val;
        }

        let preds = self.preds.get(&block_id).cloned().unwrap_or_default();
        
        if preds.is_empty() {
            // Uninitialized / Entry
            let ty = self.alloca_types.get(&var_id).unwrap();
            return if matches!(ty, Type::Float | Type::Double) {
                self.zero_float.unwrap()
            } else {
                self.zero_int.unwrap()
            };
        }
        
        if preds.len() == 1 {
            // Single predecessor: recurse and cache to avoid redundant traversals
            // on repeated lookups over long single-pred chains.
            let val = self.get_outgoing_value(preds[0], var_id);
            self.incoming_cache.insert((block_id, var_id), val);
            return val;
        }

        // Multiple preds -> Phi
        let phi_var = self.new_var();
        // Propagate the alloca's type to the phi_var so codegen knows float vs int
        if let Some(ty) = self.alloca_types.get(&var_id) {
            self.func.var_types.insert(phi_var, ty.clone());
        }
        // Break usage cycles
        self.incoming_cache.insert((block_id, var_id), phi_var);
        
        let mut phi_args = Vec::new();
        for pred in preds {
            let val = self.get_outgoing_value(pred, var_id);
            phi_args.push((pred, val));
        }
        
        // Simplify Phi
        // If all operands are the same value (or self), replace with that value
        let first = phi_args[0].1;
        let mut all_same = true;
        for (_, val) in &phi_args {
            if *val != first && *val != phi_var {
                all_same = false;
                break;
            }
        }
        
        if all_same {
            self.incoming_cache.insert((block_id, var_id), first);
            self.simplified.insert(phi_var, first);
            return first;
        }
        
        // Keep Phi
        self.block_phis.entry(block_id).or_default().push(Instruction::Phi {
            dest: phi_var,
            preds: phi_args,
        });
        
        phi_var
    }
    
    fn get_outgoing_value(&mut self, block_id: BlockId, var_id: VarId) -> VarId {
        if let Some(defs) = self.block_defs.get(&var_id) {
            if let Some(val) = defs.get(&block_id) {
                return *val;
            }
        }
        self.get_incoming_value(block_id, var_id)
    }

    fn reconstruct_blocks(&mut self) {
        for block in &mut self.func.blocks {
            if let Some(insts) = self.new_insts.remove(&block.id) {
                let mut final_insts = Vec::new();
                if let Some(phis) = self.block_phis.remove(&block.id) {
                    final_insts.extend(phis);
                }
                final_insts.extend(insts);
                block.instructions = final_insts;
            }
        }
    }

    /// Resolve a VarId through the simplification chain
    fn resolve_simplified(&self, var: VarId) -> VarId {
        let mut v = var;
        let mut seen = HashSet::new();
        while let Some(s) = self.simplified.get(&v) {
            if !seen.insert(v) { break; } // avoid infinite loops
            v = *s;
        }
        v
    }

    /// Fix up all references to simplified-away phi_vars across all instructions
    fn fixup_simplified_phi_sources(&mut self) {
        if self.simplified.is_empty() { return; }
        // Build full resolution map to avoid borrow issues
        let mut resolved_map: HashMap<VarId, VarId> = HashMap::new();
        for &var in self.simplified.keys() {
            resolved_map.insert(var, self.resolve_simplified(var));
        }
        
        let resolve_operand = |op: &mut Operand, map: &HashMap<VarId, VarId>| {
            if let Operand::Var(v) = op {
                if let Some(&resolved) = map.get(v) {
                    *v = resolved;
                }
            }
        };
        
        for block in &mut self.func.blocks {
            for instr in &mut block.instructions {
                match instr {
                    Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => {
                        resolve_operand(left, &resolved_map);
                        resolve_operand(right, &resolved_map);
                    }
                    Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => {
                        resolve_operand(src, &resolved_map);
                    }
                    Instruction::Copy { src, .. } | Instruction::Cast { src, .. } => {
                        resolve_operand(src, &resolved_map);
                    }
                    Instruction::Load { addr, .. } => {
                        resolve_operand(addr, &resolved_map);
                    }
                    Instruction::Store { addr, src, .. } => {
                        resolve_operand(addr, &resolved_map);
                        resolve_operand(src, &resolved_map);
                    }
                    Instruction::GetElementPtr { base, index, .. } => {
                        resolve_operand(base, &resolved_map);
                        resolve_operand(index, &resolved_map);
                    }
                    Instruction::Call { args, .. } => {
                        for arg in args.iter_mut() {
                            resolve_operand(arg, &resolved_map);
                        }
                    }
                    Instruction::IndirectCall { func_ptr, args, .. } => {
                        resolve_operand(func_ptr, &resolved_map);
                        for arg in args.iter_mut() {
                            resolve_operand(arg, &resolved_map);
                        }
                    }
                    Instruction::Phi { preds, .. } => {
                        for (_, src) in preds.iter_mut() {
                            if let Some(&resolved) = resolved_map.get(src) {
                                *src = resolved;
                            }
                        }
                    }
                    Instruction::VaStart { list, .. } => {
                        resolve_operand(list, &resolved_map);
                    }
                    Instruction::VaEnd { list } => {
                        resolve_operand(list, &resolved_map);
                    }
                    Instruction::VaCopy { dest, src } => {
                        resolve_operand(dest, &resolved_map);
                        resolve_operand(src, &resolved_map);
                    }
                    Instruction::VaArg { list, .. } => {
                        resolve_operand(list, &resolved_map);
                    }
                    Instruction::InlineAsm { inputs, .. } => {
                        for input in inputs.iter_mut() {
                            resolve_operand(input, &resolved_map);
                        }
                    }
                    Instruction::Alloca { .. } => {}
                }
            }
            // Also fix terminators
            match &mut block.terminator {
                Terminator::CondBr { cond, .. } => {
                    resolve_operand(cond, &resolved_map);
                }
                Terminator::Ret(Some(val)) => {
                    resolve_operand(val, &resolved_map);
                }
                _ => {}
            }
        }
    }

    fn find_max_var_id(func: &Function) -> usize {
        let mut max = 0;
        let mut check = |v: VarId| if v.0 > max { max = v.0; };
        for param in &func.params { check(param.1); }
        for block in &func.blocks {
            for instr in &block.instructions {
                 match instr {
                    Instruction::Binary { dest, .. } | Instruction::FloatBinary { dest, .. } |
                    Instruction::Unary { dest, .. } | Instruction::FloatUnary { dest, .. } |
                    Instruction::Phi { dest, .. } | Instruction::Copy { dest, .. } | Instruction::Cast { dest, .. } |
                    Instruction::Alloca { dest, .. } | Instruction::Load { dest, .. } |
                    Instruction::GetElementPtr { dest, .. } => check(*dest),
                    Instruction::Call { dest, .. } | Instruction::IndirectCall { dest, .. } => {
                        if let Some(d) = dest { check(*d) }
                    }
                    Instruction::InlineAsm { outputs, .. } => { for o in outputs { check(*o) } }
                    _ => {}
                 }
            }
        }
        max
    }

    fn new_var(&mut self) -> VarId {
        let v = VarId(self.next_var_id);
        self.next_var_id += 1;
        v
    }
    
    fn is_scalar_type(ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::UnsignedInt | Type::Char | Type::UnsignedChar |
            Type::Short | Type::UnsignedShort | Type::Long | Type::UnsignedLong |
            Type::LongLong | Type::UnsignedLongLong | 
            Type::Float | Type::Double |
            Type::Pointer(_))
    }
    
    fn is_address_taken(func: &Function, alloca_id: VarId) -> bool {
        for block in &func.blocks {
            for instr in &block.instructions {
                match instr {
                    Instruction::Load { addr: Operand::Var(id), .. } if *id == alloca_id => continue,
                    Instruction::Store { addr: Operand::Var(id), .. } if *id == alloca_id => continue,
                    _ => if Self::instruction_uses_var(instr, alloca_id) { return true; }
                }
            }
        }
        false
    }
    
    fn instruction_uses_var(instr: &Instruction, var_id: VarId) -> bool {
        match instr {
            Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => 
                Self::operand_uses_var(left, var_id) || Self::operand_uses_var(right, var_id),
            Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => 
                Self::operand_uses_var(src, var_id),
            Instruction::Copy { src, .. } | Instruction::Cast { src, .. } => Self::operand_uses_var(src, var_id),
            Instruction::Load { addr, .. } => Self::operand_uses_var(addr, var_id),
            Instruction::Store { addr, src, .. } => 
                Self::operand_uses_var(addr, var_id) || Self::operand_uses_var(src, var_id),
            Instruction::GetElementPtr { base, index, .. } => 
                Self::operand_uses_var(base, var_id) || Self::operand_uses_var(index, var_id),
            Instruction::Call { args, .. } =>
                args.iter().any(|arg| Self::operand_uses_var(arg, var_id)),
            Instruction::IndirectCall { func_ptr, args, .. } => 
                Self::operand_uses_var(func_ptr, var_id)
                || args.iter().any(|arg| Self::operand_uses_var(arg, var_id)),
            Instruction::VaStart { list, .. } => 
                Self::operand_uses_var(list, var_id),
            Instruction::VaEnd { list } => Self::operand_uses_var(list, var_id),
            Instruction::VaCopy { dest, src } => 
                Self::operand_uses_var(dest, var_id) || Self::operand_uses_var(src, var_id),
            Instruction::VaArg { list, .. } => Self::operand_uses_var(list, var_id),
            Instruction::InlineAsm { inputs, outputs, .. } => 
                inputs.iter().any(|input| Self::operand_uses_var(input, var_id))
                || outputs.contains(&var_id),
            Instruction::Phi { .. } | Instruction::Alloca { .. } => false,
        }
    }

    fn operand_uses_var(operand: &Operand, var_id: VarId) -> bool {
        matches!(operand, Operand::Var(id) if *id == var_id)
    }
}
