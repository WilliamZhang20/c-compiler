// Scalar Replacement of Aggregates (SROA)
//
// Transforms struct/array allocas into separate scalar allocas for each
// field/element. This enables mem2reg to promote each field independently,
// turning memory accesses into register operations.
//
// Algorithm:
// 1. Find all allocas of aggregate types (Struct, Union, Array)
// 2. Verify all uses of each alloca are GEPs with constant byte offsets
// 3. For each unique (alloca, offset), create a new scalar alloca
// 4. Replace all uses of GEP dest vars with the new scalar alloca vars
// 5. Remove GEP instructions and original alloca

use ir::{Function, Instruction, Operand, VarId};
use model::Type;
use std::collections::{HashMap, HashSet};

/// Run SROA on a single function.
pub fn scalar_replacement_of_aggregates(func: &mut Function) {
    // Step 1: Find all aggregate-type allocas
    let mut aggregate_allocas: Vec<(VarId, Type)> = Vec::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Alloca { dest, r#type } = inst {
                if is_aggregate_type(r#type) {
                    aggregate_allocas.push((*dest, r#type.clone()));
                }
            }
        }
    }

    if aggregate_allocas.is_empty() {
        return;
    }

    let alloca_set: HashSet<VarId> = aggregate_allocas.iter().map(|(v, _)| *v).collect();

    // Step 2: For each aggregate alloca, collect all GEPs and verify they all
    // have constant offsets. Handle chained GEPs (e.g., o.inner.a).
    // gep_to_field: maps gep_dest_var -> (root_alloca_var, total_byte_offset)
    let mut gep_to_field: HashMap<VarId, (VarId, i64)> = HashMap::new();
    // Track all uses of each alloca to verify only GEP uses
    let mut alloca_uses_ok: HashMap<VarId, bool> = alloca_set.iter().map(|v| (*v, true)).collect();

    // We may need multiple passes to resolve chained GEPs
    // First pass: collect direct GEPs from allocas
    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            for inst in &block.instructions {
                if let Instruction::GetElementPtr { dest, base: Operand::Var(base_var), index, element_type } = inst {
                    if gep_to_field.contains_key(dest) {
                        continue; // Already resolved
                    }
                    if let Operand::Constant(offset) = index {
                        let byte_offset = if *element_type == Type::Char || *element_type == Type::UnsignedChar {
                            *offset
                        } else {
                            let stride = type_size(element_type);
                            *offset * stride as i64
                        };

                        if alloca_set.contains(base_var) {
                            // Direct GEP from alloca
                            gep_to_field.insert(*dest, (*base_var, byte_offset));
                            changed = true;
                        } else if let Some((root_alloca, parent_offset)) = gep_to_field.get(base_var).copied() {
                            // Chained GEP: compose offsets
                            gep_to_field.insert(*dest, (root_alloca, parent_offset + byte_offset));
                            changed = true;
                        }
                    } else if alloca_set.contains(base_var) {
                        // Non-constant index from alloca — disqualify
                        alloca_uses_ok.insert(*base_var, false);
                    }
                }
            }
        }
    }

    // Check for non-GEP uses of allocas, and also check that intermediate
    // GEP results (used as bases for further GEPs) are not used directly
    // by Load/Store or other instructions
    let gep_vars: HashSet<VarId> = gep_to_field.keys().copied().collect();
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::GetElementPtr { .. } | Instruction::Alloca { .. } => {}
                _ => {
                    // Check if alloca vars are used directly
                    check_direct_use(inst, &alloca_set, &mut alloca_uses_ok);
                    // Check if intermediate GEP vars are used as bases for non-GEP operations
                    // (which would mean we can't just replace the GEP)
                    // This is OK — we handle it by keeping GEPs that are "leaf" GEPs
                    // (used by Load/Store) and replacing them.
                }
            }
        }
    }

    // Filter out allocas that have non-GEP uses or variable-index GEPs
    let eligible_allocas: HashSet<VarId> = alloca_uses_ok.iter()
        .filter(|(_, ok)| **ok)
        .map(|(v, _)| *v)
        .collect();

    if eligible_allocas.is_empty() {
        return;
    }

    // Remove GEPs from ineligible allocas
    gep_to_field.retain(|_, (alloca_var, _)| eligible_allocas.contains(alloca_var));

    if gep_to_field.is_empty() {
        return;
    }

    // Identify "leaf" GEPs (those used by Load/Store/Call, not just as bases for other GEPs)
    // and "intermediate" GEPs (those only used as bases for other GEPs).
    // Only leaf GEPs need substitution; intermediate GEPs just get removed.
    let mut gep_used_as_base: HashSet<VarId> = HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::GetElementPtr { base: Operand::Var(base_var), .. } = inst {
                if gep_to_field.contains_key(base_var) {
                    gep_used_as_base.insert(*base_var);
                }
            }
        }
    }

    // Safety check: if any intermediate GEP var is used in a context other
    // than as a GEP base (e.g., as a call argument, store source, etc.),
    // disqualify the root alloca. We can only SROA if all non-leaf GEP
    // results are used exclusively as bases for further GEPs.
    let mut disqualified_allocas: HashSet<VarId> = HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::GetElementPtr { .. } | Instruction::Alloca { .. } => continue,
                _ => {}
            }
            // Check operands: if any operand is an intermediate GEP var,
            // and the instruction is not a Load/Store (OK for leaf GEPs),
            // then we need to check if it's a leaf or intermediate GEP.
            inst.for_each_use(|v| {
                if let Some((root_alloca, _)) = gep_to_field.get(&v) {
                    if gep_used_as_base.contains(&v) {
                        // Intermediate GEP used in non-GEP context
                        // (e.g., passed as call argument for by-value struct)
                        disqualified_allocas.insert(*root_alloca);
                    }
                }
            });
        }
    }

    // Remove disqualified allocas
    if !disqualified_allocas.is_empty() {
        gep_to_field.retain(|_, (alloca_var, _)| !disqualified_allocas.contains(alloca_var));
        if gep_to_field.is_empty() {
            return;
        }
    }

    // Determine field types from Load/Store instructions
    // field_key: (alloca_var, byte_offset) -> field_type
    let mut field_types: HashMap<(VarId, i64), Type> = HashMap::new();

    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Load { addr: Operand::Var(addr_var), value_type, .. } => {
                    if let Some((alloca_var, offset)) = gep_to_field.get(addr_var) {
                        // If any Load/Store at this offset uses an aggregate type,
                        // disqualify the alloca (we can't represent sub-struct
                        // access after breaking into scalar allocas)
                        if is_aggregate_type(value_type) {
                            disqualified_allocas.insert(*alloca_var);
                        } else {
                            field_types.entry((*alloca_var, *offset))
                                .or_insert_with(|| value_type.clone());
                        }
                    }
                }
                Instruction::Store { addr: Operand::Var(addr_var), value_type, .. } => {
                    if let Some((alloca_var, offset)) = gep_to_field.get(addr_var) {
                        if is_aggregate_type(value_type) {
                            disqualified_allocas.insert(*alloca_var);
                        } else {
                            field_types.entry((*alloca_var, *offset))
                                .or_insert_with(|| value_type.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Remove allocas disqualified during field type collection
    if !disqualified_allocas.is_empty() {
        gep_to_field.retain(|_, (alloca_var, _)| !disqualified_allocas.contains(alloca_var));
        if gep_to_field.is_empty() {
            return;
        }
    }

    // Step 4: Create new scalar allocas for each field
    // Find max VarId
    let mut max_var = 0usize;
    for (v, _) in &func.var_types {
        max_var = max_var.max(v.0);
    }
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Some(d) = inst.dest() {
                max_var = max_var.max(d.0);
            }
        }
    }
    let mut next_var = max_var + 1;

    // field_alloca: (alloca_var, byte_offset) -> new_scalar_alloca_var
    let mut field_alloca: HashMap<(VarId, i64), VarId> = HashMap::new();
    let mut new_allocas: Vec<Instruction> = Vec::new();

    for ((alloca_var, offset), field_type) in &field_types {
        let new_var = VarId(next_var);
        next_var += 1;
        field_alloca.insert((*alloca_var, *offset), new_var);
        new_allocas.push(Instruction::Alloca {
            dest: new_var,
            r#type: field_type.clone(),
        });
        // Register the type
        func.var_types.insert(new_var, Type::Pointer(Box::new(field_type.clone()), model::TypeQualifiers::default()));
    }

    // Step 5: Build substitution map: gep_dest_var -> new_scalar_alloca_var
    let mut subst: HashMap<VarId, VarId> = HashMap::new();
    for (gep_dest, (alloca_var, offset)) in &gep_to_field {
        if let Some(new_var) = field_alloca.get(&(*alloca_var, *offset)) {
            subst.insert(*gep_dest, *new_var);
        }
    }

    if subst.is_empty() {
        return;
    }

    // Step 6: Insert new allocas at the start of the entry block
    if let Some(entry) = func.blocks.first_mut() {
        for (i, alloca) in new_allocas.into_iter().enumerate() {
            entry.instructions.insert(i, alloca);
        }
    }

    // Step 7: Remove GEP instructions for eligible allocas, and apply
    // variable substitution throughout the function
    let gep_dests: HashSet<VarId> = gep_to_field.keys().copied().collect();

    for block in &mut func.blocks {
        // Remove GEPs that we're replacing (including chained GEPs where
        // the base is another GEP, not directly the alloca)
        block.instructions.retain(|inst| {
            if let Instruction::GetElementPtr { dest, .. } = inst {
                if gep_dests.contains(dest) {
                    return false; // Remove this GEP
                }
            }
            true
        });

        // Remove original aggregate allocas
        block.instructions.retain(|inst| {
            if let Instruction::Alloca { dest, .. } = inst {
                if eligible_allocas.contains(dest) {
                    return false;
                }
            }
            true
        });

        // Apply substitution to all remaining instructions
        for inst in &mut block.instructions {
            substitute_vars_in_instruction(inst, &subst);
        }

        // Apply substitution to terminator
        substitute_vars_in_terminator(&mut block.terminator, &subst);
    }
}

fn is_aggregate_type(ty: &Type) -> bool {
    matches!(ty, Type::Struct(_) | Type::Union(_) | Type::Array(..))
}

fn type_size(ty: &Type) -> usize {
    match ty {
        Type::Char | Type::UnsignedChar | Type::Bool => 1,
        Type::Short | Type::UnsignedShort => 2,
        Type::Int | Type::UnsignedInt | Type::Float | Type::Enum(_) => 4,
        Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong
        | Type::Double | Type::Pointer(..) | Type::FunctionPointer { .. } => 8,
        _ => 1,
    }
}

/// Check if an instruction directly uses an alloca var (not through GEP).
/// If so, mark the alloca as ineligible.
fn check_direct_use(inst: &Instruction, alloca_set: &HashSet<VarId>, ok_map: &mut HashMap<VarId, bool>) {
    match inst {
        // Alloca definitions are fine
        Instruction::Alloca { .. } => {}
        // GEPs are handled separately
        Instruction::GetElementPtr { .. } => {}
        _ => {
            inst.for_each_use(|v| {
                if alloca_set.contains(&v) {
                    ok_map.insert(v, false);
                }
            });
        }
    }
}

fn substitute_vars_in_operand(op: &mut Operand, subst: &HashMap<VarId, VarId>) {
    if let Operand::Var(v) = op {
        if let Some(new_v) = subst.get(v) {
            *v = *new_v;
        }
    }
}

fn substitute_vars_in_instruction(inst: &mut Instruction, subst: &HashMap<VarId, VarId>) {
    match inst {
        Instruction::Binary { dest, left, right, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(left, subst);
            substitute_vars_in_operand(right, subst);
        }
        Instruction::FloatBinary { dest, left, right, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(left, subst);
            substitute_vars_in_operand(right, subst);
        }
        Instruction::Unary { dest, src, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(src, subst);
        }
        Instruction::FloatUnary { dest, src, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(src, subst);
        }
        Instruction::Copy { dest, src } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(src, subst);
        }
        Instruction::Cast { dest, src, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(src, subst);
        }
        Instruction::Alloca { dest, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
        }
        Instruction::Load { dest, addr, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(addr, subst);
        }
        Instruction::Store { addr, src, .. } => {
            substitute_vars_in_operand(addr, subst);
            substitute_vars_in_operand(src, subst);
        }
        Instruction::GetElementPtr { dest, base, index, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(base, subst);
            substitute_vars_in_operand(index, subst);
        }
        Instruction::Call { dest, args, .. } => {
            if let Some(d) = dest {
                if let Some(nv) = subst.get(d) { *d = *nv; }
            }
            for arg in args.iter_mut() {
                substitute_vars_in_operand(arg, subst);
            }
        }
        Instruction::IndirectCall { dest, func_ptr, args, .. } => {
            if let Some(d) = dest {
                if let Some(nv) = subst.get(d) { *d = *nv; }
            }
            substitute_vars_in_operand(func_ptr, subst);
            for arg in args.iter_mut() {
                substitute_vars_in_operand(arg, subst);
            }
        }
        Instruction::Phi { dest, preds } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            for (_, v) in preds.iter_mut() {
                if let Some(nv) = subst.get(v) { *v = *nv; }
            }
        }
        Instruction::VaStart { list, .. } => {
            substitute_vars_in_operand(list, subst);
        }
        Instruction::VaEnd { list } => {
            substitute_vars_in_operand(list, subst);
        }
        Instruction::VaCopy { dest, src } => {
            substitute_vars_in_operand(dest, subst);
            substitute_vars_in_operand(src, subst);
        }
        Instruction::VaArg { dest, list, .. } => {
            if let Some(nv) = subst.get(dest) { *dest = *nv; }
            substitute_vars_in_operand(list, subst);
        }
        Instruction::InlineAsm { outputs, inputs, .. } => {
            for o in outputs.iter_mut() {
                if let Some(nv) = subst.get(o) { *o = *nv; }
            }
            for inp in inputs.iter_mut() {
                substitute_vars_in_operand(inp, subst);
            }
        }
        Instruction::Simd { dest, operands, .. } => {
            if let Some(d) = dest {
                if let Some(nv) = subst.get(d) { *d = *nv; }
            }
            for op in operands.iter_mut() {
                substitute_vars_in_operand(op, subst);
            }
        }
    }
}

fn substitute_vars_in_terminator(term: &mut ir::Terminator, subst: &HashMap<VarId, VarId>) {
    match term {
        ir::Terminator::Ret(Some(op)) => {
            substitute_vars_in_operand(op, subst);
        }
        ir::Terminator::CondBr { cond, .. } => {
            substitute_vars_in_operand(cond, subst);
        }
        _ => {}
    }
}
