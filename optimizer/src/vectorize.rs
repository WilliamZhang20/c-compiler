// Auto-vectorization pass
//
// Transforms scalar loops into SIMD vector operations when safe.
// Supports SSE (128-bit, 4x float/int32) and AVX2 (256-bit, 8x float/int32).
//
// The pass works at the IR level:
// 1. Find natural loops with known trip counts
// 2. Analyze memory access patterns for consecutive loads/stores
// 3. Check for loop-carried dependencies
// 4. Generate vectorized IR with explicit vector width annotations
//
// The codegen then maps VectorLoad/VectorStore/VectorOp instructions to
// packed SSE or AVX instructions based on the target features.

use ir::{Function, Instruction, Operand, VarId, BlockId, Terminator, BasicBlock, SimdOp};
use model::{BinaryOp, Type};
use std::collections::{HashMap, HashSet};
use crate::loop_analysis::{self, NaturalLoop};

/// Target SIMD capability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    /// SSE2: 128-bit registers, 4x float or 4x i32
    SSE2,
    /// AVX2: 256-bit registers, 8x float or 8x i32
    AVX2,
}

impl SimdLevel {
    /// Number of 32-bit elements that fit in a vector register
    pub fn vector_width(self) -> usize {
        match self {
            SimdLevel::SSE2 => 4,
            SimdLevel::AVX2 => 8,
        }
    }
}

/// Detect the SIMD level supported by the current CPU
pub fn detect_simd_level() -> SimdLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return SimdLevel::AVX2;
        }
    }
    SimdLevel::SSE2 // SSE2 is baseline for x86-64
}

/// Information about a vectorizable memory access in a loop
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MemAccess {
    /// The IR variable holding the array base address
    base_var: VarId,
    /// The induction variable used for indexing
    index_var: VarId,
    /// Element type
    elem_type: Type,
    /// Whether this is a load (true) or store (false)
    is_load: bool,
    /// The destination variable (for loads) or source operand (for stores)
    data: Operand,
    /// The destination var for loads
    dest: Option<VarId>,
}

/// Describes a vectorizable reduction pattern (e.g., sum += a[i])
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Reduction {
    /// The accumulator variable (phi node in header)
    accum_var: VarId,
    /// The binary operation for accumulation
    op: BinaryOp,
    /// Initial value
    init: Operand,
    /// Whether this is a float reduction
    is_float: bool,
}

/// Check if a loop body contains only vectorizable operations
fn analyze_loop_body(
    func: &Function,
    lp: &NaturalLoop,
) -> Option<VectorizationPlan> {
    let iv = lp.induction_var.as_ref()?;
    let trip_count = lp.trip_count?;

    if trip_count < 4 {
        return None; // Too small to vectorize
    }

    let mut loads: Vec<MemAccess> = Vec::new();
    let mut stores: Vec<MemAccess> = Vec::new();
    let mut reductions: Vec<Reduction> = Vec::new();
    let mut arithmetic_ops: Vec<(VarId, BinaryOp, Operand, Operand, bool)> = Vec::new();
    let mut has_calls = false;
    let mut has_complex_control_flow = false;

    // Check for complex control flow (nested branches in loop body)
    for &block_id in &lp.body {
        if block_id == lp.header {
            continue;
        }
        let block = func.blocks.iter().find(|b| b.id == block_id)?;
        match &block.terminator {
            Terminator::CondBr { .. } => {
                has_complex_control_flow = true;
            }
            _ => {}
        }
    }

    if has_complex_control_flow {
        return None; // Can't vectorize loops with internal branches (yet)
    }

    // Collect all phi nodes in the header to find reductions
    let header_block = func.blocks.iter().find(|b| b.id == lp.header)?;
    let mut phi_vars: HashMap<VarId, Vec<(BlockId, VarId)>> = HashMap::new();
    for inst in &header_block.instructions {
        if let Instruction::Phi { dest, preds } = inst {
            if *dest != iv.var {
                phi_vars.insert(*dest, preds.clone());
            }
        }
    }

    // Analyze body blocks
    for &block_id in &lp.body {
        let block = func.blocks.iter().find(|b| b.id == block_id)?;
        for inst in &block.instructions {
            match inst {
                Instruction::Call { .. } | Instruction::IndirectCall { .. } => {
                    has_calls = true;
                }
                Instruction::GetElementPtr { dest: _, base: _, index, element_type: _ } => {
                    // Check if the index is the induction variable or derived from it
                    if is_iv_derived(index, iv.var) {
                        // This GEP uses the IV for array indexing - potentially vectorizable
                    }
                }
                Instruction::Load { dest, addr, value_type } => {
                    // Check if this loads from an IV-indexed GEP
                    if let Operand::Var(addr_var) = addr {
                        if let Some(gep_info) = find_gep_for_var(func, *addr_var, &lp.body, iv.var) {
                            loads.push(MemAccess {
                                base_var: gep_info.0,
                                index_var: iv.var,
                                elem_type: value_type.clone(),
                                is_load: true,
                                data: Operand::Var(*dest),
                                dest: Some(*dest),
                            });
                        }
                    }
                }
                Instruction::Store { addr, src, value_type } => {
                    if let Operand::Var(addr_var) = addr {
                        if let Some(gep_info) = find_gep_for_var(func, *addr_var, &lp.body, iv.var) {
                            stores.push(MemAccess {
                                base_var: gep_info.0,
                                index_var: iv.var,
                                elem_type: value_type.clone(),
                                is_load: false,
                                data: src.clone(),
                                dest: None,
                            });
                        }
                    }
                }
                Instruction::Binary { dest, op, left, right } => {
                    arithmetic_ops.push((*dest, op.clone(), left.clone(), right.clone(), false));
                }
                Instruction::FloatBinary { dest, op, left, right } => {
                    arithmetic_ops.push((*dest, op.clone(), left.clone(), right.clone(), true));
                }
                Instruction::Phi { .. } => {} // Handled above
                Instruction::Copy { .. } => {} // OK
                _ => {}
            }
        }
    }

    if has_calls {
        return None; // Can't vectorize loops with function calls
    }

    // Check for simple patterns:
    // 1. Array copy: a[i] = b[i]
    // 2. Array op: c[i] = a[i] op b[i]
    // 3. Reduction: sum += a[i]

    // Detect reductions (phi var updated with binary op involving loaded value)
    for (phi_var, phi_preds) in &phi_vars {
        // Find the in-loop update of the phi var
        for (pred_block, pred_var) in phi_preds {
            if lp.body.contains(pred_block) {
                // Find what produces pred_var in the body
                for &(dest, ref op, ref left, ref right, is_float) in &arithmetic_ops {
                    if dest == *pred_var {
                        // Check if one operand is the phi var and the other involves a loaded value
                        let uses_phi = matches!(left, Operand::Var(v) if *v == *phi_var)
                            || matches!(right, Operand::Var(v) if *v == *phi_var);
                        if uses_phi && matches!(op, BinaryOp::Add | BinaryOp::Mul) {
                            // Find initial value from outside the loop
                            let init_val = phi_preds.iter()
                                .find(|(b, _)| !lp.body.contains(b))
                                .map(|(_, v)| Operand::Var(*v));
                            if let Some(init) = init_val {
                                reductions.push(Reduction {
                                    accum_var: *phi_var,
                                    op: op.clone(),
                                    init,
                                    is_float,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Check for read-after-write or write-after-read dependencies between different arrays
    // (same array read and write at same index is OK only if no other element deps)
    if !check_memory_safety(&loads, &stores) {
        return None;
    }

    // We have a vectorizable loop if we have loads/stores or reductions
    if loads.is_empty() && stores.is_empty() && reductions.is_empty() {
        return None;
    }

    // Check for stores whose data depends on the induction variable (e.g., arr[i] = i).
    // A Splat would incorrectly broadcast a single IV value to all lanes.
    // We can only vectorize stores whose data is either:
    //   - A loop-invariant scalar (correct to splat), or
    //   - Derived from a vector load (will be remapped to a vector register)
    let load_vars: HashSet<VarId> = loads.iter().filter_map(|l| l.dest).collect();
    for store in &stores {
        if is_iv_or_iv_derived(&store.data, iv.var, &arithmetic_ops, &load_vars) {
            return None;
        }
    }

    Some(VectorizationPlan {
        trip_count,
        loads,
        stores,
        reductions,
        arithmetic_ops,
    })
}

/// Check if an operand is the induction variable or derived from it
fn is_iv_derived(op: &Operand, iv_var: VarId) -> bool {
    match op {
        Operand::Var(v) => *v == iv_var,
        _ => false,
    }
}

/// Check if an operand depends on the induction variable, either directly or
/// through arithmetic operations, but NOT through vector loads (which are safely
/// remapped during vectorization).
fn is_iv_or_iv_derived(
    op: &Operand,
    iv_var: VarId,
    arith_ops: &[(VarId, BinaryOp, Operand, Operand, bool)],
    load_vars: &HashSet<VarId>,
) -> bool {
    match op {
        Operand::Var(v) => {
            if *v == iv_var {
                return true;
            }
            // If this variable comes from a load, it will be vectorized properly
            if load_vars.contains(v) {
                return false;
            }
            // Check if produced by an arithmetic op that depends on the IV
            for (dest, _, left, right, _) in arith_ops {
                if *dest == *v {
                    return is_iv_or_iv_derived(left, iv_var, arith_ops, load_vars)
                        || is_iv_or_iv_derived(right, iv_var, arith_ops, load_vars);
                }
            }
            false
        }
        Operand::Constant(_) | Operand::FloatConstant(_) | Operand::Global(_) => false,
    }
}

/// Find the GEP instruction that produces a given variable, returning (base_var, index_var)
fn find_gep_for_var(
    func: &Function,
    var: VarId,
    body: &HashSet<BlockId>,
    iv_var: VarId,
) -> Option<(VarId, VarId)> {
    for &block_id in body {
        if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
            for inst in &block.instructions {
                if let Instruction::GetElementPtr { dest, base, index, .. } = inst {
                    if *dest == var {
                        if let Operand::Var(base_v) = base {
                            if is_iv_derived(index, iv_var) {
                                return Some((*base_v, iv_var));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check memory safety: no overlapping writes to reads
fn check_memory_safety(loads: &[MemAccess], stores: &[MemAccess]) -> bool {
    // Simple check: different base arrays for loads and stores, OR
    // same base but read and write at same index (element-wise)
    for store in stores {
        for load in loads {
            if store.base_var == load.base_var {
                // Same array - only safe if writing and reading same element
                // (which is a copy/transform pattern: a[i] = f(a[i]))
                // For now, allow this common pattern
            }
        }
        // Check write-after-write at different bases
        for other_store in stores {
            if store.base_var == other_store.base_var && !std::ptr::eq(store, other_store) {
                return false; // Two stores to same array - unsafe
            }
        }
    }
    true
}

/// Plan for vectorizing a loop
#[derive(Debug)]
struct VectorizationPlan {
    trip_count: usize,
    loads: Vec<MemAccess>,
    stores: Vec<MemAccess>,
    reductions: Vec<Reduction>,
    arithmetic_ops: Vec<(VarId, BinaryOp, Operand, Operand, bool)>,
}

/// Main auto-vectorization entry point
/// Analyzes loops and inserts vector IR annotations that codegen will use
pub fn vectorize_function(func: &mut Function, simd_level: SimdLevel) {
    let loops = loop_analysis::find_loops(func);

    for lp in &loops {
        if let Some(plan) = analyze_loop_body(func, lp) {
            let vf = simd_level.vector_width();
            if plan.trip_count >= vf {
                apply_vectorization(func, lp, &plan, simd_level);
            }
        }
    }
}

/// Apply vectorization to a loop by transforming it into a vectorized + remainder structure.
///
/// Original: for (i = 0; i < N; i++) body(i)
/// Becomes:  for (i = 0; i < N - N%VF; i += VF) vector_body(i)
///           for (; i < N; i++) body(i)  // remainder (original loop)
///
/// Correctness requirements:
/// 1. The vectorized header uses a proper Phi node for the IV
/// 2. The IV is incremented by VF each iteration in the vectorized body
/// 3. After the vectorized loop exits, control flows to the original loop
///    whose IV starts at vec_trip_count (for remainder iterations)
/// 4. The vectorized body emits proper Simd IR instructions
fn apply_vectorization(
    func: &mut Function,
    lp: &NaturalLoop,
    plan: &VectorizationPlan,
    simd_level: SimdLevel,
) {
    let vf = simd_level.vector_width();
    let iv = match &lp.induction_var {
        Some(iv) => iv,
        None => return,
    };

    let trip_count = match lp.trip_count {
        Some(tc) => tc,
        None => return,
    };

    // Only vectorize if we have enough iterations and actual memory operations
    let vec_iters = trip_count / vf;
    if vec_iters == 0 {
        return;
    }

    // Must have array memory accesses to vectorize
    if plan.loads.is_empty() && plan.stores.is_empty() {
        return;
    }

    let vec_trip_count = vec_iters * vf;
    let has_remainder = trip_count % vf != 0;

    // We need a preheader to redirect
    let preheader = match lp.preheader {
        Some(p) => p,
        None => return,
    };
    let exit_block = match lp.exit {
        Some(e) => e,
        None => return,
    };

    // Create new block IDs
    let max_block_id = func.blocks.iter().map(|b| b.id.0).max().unwrap_or(0);
    let max_var_id = find_max_var_id(func);

    let vec_header_id = BlockId(max_block_id + 1);
    let vec_body_id = BlockId(max_block_id + 2);

    // Variables for the vectorized loop
    let vec_iv = VarId(max_var_id + 1);          // Phi-merged IV for vec loop
    let vec_iv_next = VarId(max_var_id + 2);     // IV after increment by VF
    let vec_cmp = VarId(max_var_id + 3);         // comparison result
    let vec_init_iv = VarId(max_var_id + 4);     // copy of init value for phi source
    let mut next_var = max_var_id + 5;

    // --- Build vectorized body instructions ---
    let mut vec_body_insts: Vec<Instruction> = Vec::new();

    // Determine the element type for SIMD instructions
    let _elem_type = if !plan.loads.is_empty() {
        plan.loads[0].elem_type.clone()
    } else if !plan.stores.is_empty() {
        plan.stores[0].elem_type.clone()
    } else {
        Type::Int
    };

    // For each load, generate: GEP + Simd::Load
    let mut load_vec_vars: HashMap<VarId, VarId> = HashMap::new();
    for load in &plan.loads {
        let gep_dest = VarId(next_var);
        next_var += 1;
        let vec_load_dest = VarId(next_var);
        next_var += 1;

        // GEP with vectorized IV
        vec_body_insts.push(Instruction::GetElementPtr {
            dest: gep_dest,
            base: Operand::Var(load.base_var),
            index: Operand::Var(vec_iv),
            element_type: load.elem_type.clone(),
        });

        // Vector load
        vec_body_insts.push(Instruction::Simd {
            op: SimdOp::Load,
            dest: Some(vec_load_dest),
            operands: vec![Operand::Var(gep_dest)],
            elem_type: load.elem_type.clone(),
            width: vf,
        });

        if let Some(orig_dest) = load.dest {
            load_vec_vars.insert(orig_dest, vec_load_dest);
        }
    }

    // Collect reduction accumulator variables — we can't vectorize these yet
    // (would need vector accumulators + horizontal reduction)
    let reduction_accums: HashSet<VarId> = plan.reductions.iter()
        .map(|r| r.accum_var)
        .collect();

    // For each arithmetic op that works on vector data, generate Simd binary op
    let mut op_vec_vars: HashMap<VarId, VarId> = HashMap::new();
    for &(dest, ref op, ref left, ref right, is_float) in &plan.arithmetic_ops {
        // Skip IV increment and comparison
        if dest == iv.var {
            continue;
        }

        // Skip ops involving reduction accumulators (not yet vectorizable)
        let left_is_accum = matches!(left, Operand::Var(v) if reduction_accums.contains(v));
        let right_is_accum = matches!(right, Operand::Var(v) if reduction_accums.contains(v));
        if left_is_accum || right_is_accum {
            continue;
        }

        let left_is_vec = match left {
            Operand::Var(v) => load_vec_vars.contains_key(v) || op_vec_vars.contains_key(v),
            _ => false,
        };
        let right_is_vec = match right {
            Operand::Var(v) => load_vec_vars.contains_key(v) || op_vec_vars.contains_key(v),
            _ => false,
        };

        if !left_is_vec && !right_is_vec {
            continue;
        }

        let simd_op = match op {
            BinaryOp::Add => SimdOp::Add,
            BinaryOp::Sub => SimdOp::Sub,
            BinaryOp::Mul => SimdOp::Mul,
            _ => continue,  // Skip non-vectorizable ops
        };

        let vec_left = remap_vec_operand(left, &load_vec_vars, &op_vec_vars);
        let vec_right = remap_vec_operand(right, &load_vec_vars, &op_vec_vars);

        let vec_dest = VarId(next_var);
        next_var += 1;

        let op_elem_type = if is_float { Type::Float } else { Type::Int };
        vec_body_insts.push(Instruction::Simd {
            op: simd_op,
            dest: Some(vec_dest),
            operands: vec![vec_left, vec_right],
            elem_type: op_elem_type,
            width: vf,
        });

        op_vec_vars.insert(dest, vec_dest);
    }

    // For each store, generate: GEP + Simd::Store
    // If the store data is scalar (not from a vector load/op), insert a Splat
    // to broadcast the scalar to all lanes before the vector store.
    let mut valid_simd_stores = 0;
    for store in &plan.stores {
        let vec_src = remap_vec_operand(&store.data, &load_vec_vars, &op_vec_vars);

        // Check if the store data was remapped to a vector value
        let is_remapped = match (&store.data, &vec_src) {
            (Operand::Var(orig), Operand::Var(mapped)) => orig != mapped,
            _ => false,
        };

        // If scalar, insert a Splat to broadcast the value to all vector lanes
        let final_vec_src = if !is_remapped {
            let splat_dest = VarId(next_var);
            next_var += 1;
            vec_body_insts.push(Instruction::Simd {
                op: SimdOp::Splat,
                dest: Some(splat_dest),
                operands: vec![store.data.clone()],
                elem_type: store.elem_type.clone(),
                width: vf,
            });
            Operand::Var(splat_dest)
        } else {
            vec_src
        };

        let gep_dest = VarId(next_var);
        next_var += 1;

        vec_body_insts.push(Instruction::GetElementPtr {
            dest: gep_dest,
            base: Operand::Var(store.base_var),
            index: Operand::Var(vec_iv),
            element_type: store.elem_type.clone(),
        });

        vec_body_insts.push(Instruction::Simd {
            op: SimdOp::Store,
            dest: None,
            operands: vec![Operand::Var(gep_dest), final_vec_src],
            elem_type: store.elem_type.clone(),
            width: vf,
        });

        valid_simd_stores += 1;
    }

    // Bail out if no useful SIMD work was generated.
    // We need at least one SIMD store (map pattern) to justify vectorization.
    // Pure loads without stores/reductions are useless.
    if valid_simd_stores == 0 {
        return;
    }

    // IV increment: vec_iv_next = vec_iv + VF
    vec_body_insts.push(Instruction::Binary {
        dest: vec_iv_next,
        op: BinaryOp::Add,
        left: Operand::Var(vec_iv),
        right: Operand::Constant(vf as i64),
    });

    // --- Build the vectorized loop header ---
    // Uses a properly-formed Phi node for the IV
    let vec_header = BasicBlock {
        id: vec_header_id,
        instructions: vec![
            // Phi: vec_iv comes from preheader (init) or vec_body (vec_iv_next)
            Instruction::Phi {
                dest: vec_iv,
                preds: vec![
                    (preheader, vec_init_iv),
                    (vec_body_id, vec_iv_next),
                ],
            },
            // Compare: vec_iv < vec_trip_count
            Instruction::Binary {
                dest: vec_cmp,
                op: BinaryOp::Less,
                left: Operand::Var(vec_iv),
                right: Operand::Constant(vec_trip_count as i64),
            },
        ],
        terminator: Terminator::CondBr {
            cond: Operand::Var(vec_cmp),
            then_block: vec_body_id,
            else_block: if has_remainder { lp.header } else { exit_block },
        },
        is_label_target: false,
    };

    // --- Build the vectorized loop body ---
    let vec_body = BasicBlock {
        id: vec_body_id,
        instructions: vec_body_insts,
        terminator: Terminator::Br(vec_header_id),
        is_label_target: false,
    };

    // --- Modify preheader to jump to vectorized header ---
    // Also add a Copy for the init value that the phi will reference
    if let Some(pre_block) = func.blocks.iter_mut().find(|b| b.id == preheader) {
        // Add init copy for vec loop phi
        pre_block.instructions.push(Instruction::Copy {
            dest: vec_init_iv,
            src: Operand::Constant(iv.init),
        });

        // Redirect preheader to vec_header
        match &mut pre_block.terminator {
            Terminator::Br(target) if *target == lp.header => {
                *target = vec_header_id;
            }
            Terminator::CondBr { then_block, else_block, .. } => {
                if *then_block == lp.header { *then_block = vec_header_id; }
                if *else_block == lp.header { *else_block = vec_header_id; }
            }
            _ => {}
        }
    }

    // --- Handle remainder: update original loop's IV init ---
    if has_remainder {
        // The original loop's IV phi gets its init from preheader.
        // After vectorization, when we fall through from vec_header → original header,
        // the coming-from block is vec_header_id (not preheader anymore).
        // We need to update the original loop's Phi to accept vec_header_id → vec_trip_count.
        if let Some(header_block) = func.blocks.iter_mut().find(|b| b.id == lp.header) {
            for inst in &mut header_block.instructions {
                if let Instruction::Phi { dest, preds } = inst {
                    if *dest == iv.var {
                        // Change the preheader source to come from vec_header instead
                        for (pred_block, _pred_var) in preds.iter_mut() {
                            if *pred_block == preheader {
                                *pred_block = vec_header_id;
                                // Create a new var holding vec_trip_count
                                // We'll add a Copy in vec_header for this
                            }
                        }
                    }
                }
            }
        }
        // For the phi update: the vec_header already defines vec_iv which equals
        // vec_trip_count when the loop exits. So we can use vec_iv as the init.
        // But the phi currently references some VarId from preheader.
        // We need to create a new var that holds vec_trip_count in vec_header.
        // Actually, when vec_header exits to lp.header, vec_iv holds vec_trip_count
        // (since the condition vec_iv < vec_trip_count was false).
        // So we update the phi to use vec_iv from vec_header_id.
        if let Some(header_block) = func.blocks.iter_mut().find(|b| b.id == lp.header) {
            for inst in &mut header_block.instructions {
                if let Instruction::Phi { dest, preds } = inst {
                    if *dest == iv.var {
                        for (pred_block, pred_var) in preds.iter_mut() {
                            if *pred_block == vec_header_id {
                                *pred_var = vec_iv;
                            }
                        }
                    }
                }
            }
        }
    } else {
        // No remainder: vec_header exits directly to exit_block
        // The original loop still exists but is now unreachable
        // (preheader → vec_header → exit_block, skipping original header)
    }

    // Add the new blocks (insert before original loop blocks for better layout)
    func.blocks.push(vec_header);
    func.blocks.push(vec_body);
}

fn find_max_var_id(func: &Function) -> usize {
    let mut max = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Binary { dest, .. } | Instruction::FloatBinary { dest, .. } |
                Instruction::Unary { dest, .. } | Instruction::FloatUnary { dest, .. } |
                Instruction::Copy { dest, .. } | Instruction::Cast { dest, .. } |
                Instruction::Load { dest, .. } | Instruction::GetElementPtr { dest, .. } |
                Instruction::Alloca { dest, .. } | Instruction::Phi { dest, .. } => {
                    max = max.max(dest.0);
                }
                Instruction::Call { dest: Some(d), .. } | Instruction::IndirectCall { dest: Some(d), .. } |
                Instruction::VaArg { dest: d, .. } => {
                    max = max.max(d.0);
                }
                Instruction::InlineAsm { outputs, .. } => {
                    for o in outputs {
                        max = max.max(o.0);
                    }
                }
                Instruction::Simd { dest: Some(d), .. } => {
                    max = max.max(d.0);
                }
                _ => {}
            }
        }
    }
    max
}

fn remap_vec_operand(op: &Operand, load_map: &HashMap<VarId, VarId>, op_map: &HashMap<VarId, VarId>) -> Operand {
    match op {
        Operand::Var(v) => {
            if let Some(mapped) = load_map.get(v).or_else(|| op_map.get(v)) {
                Operand::Var(*mapped)
            } else {
                op.clone()
            }
        }
        _ => op.clone(),
    }
}

fn _type_str(ty: &Type) -> &'static str {
    match ty {
        Type::Float => "float",
        Type::Double => "double",
        Type::Int => "int32",
        _ => "int32",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_func(blocks: Vec<BasicBlock>) -> Function {
        Function {
            name: "test".to_string(),
            return_type: Type::Int,
            params: vec![],
            entry_block: BlockId(0),
            blocks,
            var_types: HashMap::new(),
            attributes: vec![],
            is_static: false,
        }
    }

    fn ret_block(id: usize) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            instructions: vec![],
            terminator: Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }
    }

    #[test]
    fn test_detect_simd() {
        let level = detect_simd_level();
        // Should return at least SSE2 on x86-64
        assert!(matches!(level, SimdLevel::SSE2 | SimdLevel::AVX2));
    }

    #[test]
    fn test_simd_vector_width() {
        assert_eq!(SimdLevel::SSE2.vector_width(), 4);
        assert_eq!(SimdLevel::AVX2.vector_width(), 8);
    }

    // ─── is_iv_derived ──────────────────────────────────────────

    #[test]
    fn test_is_iv_derived_var_match() {
        assert!(is_iv_derived(&Operand::Var(VarId(5)), VarId(5)));
    }

    #[test]
    fn test_is_iv_derived_var_no_match() {
        assert!(!is_iv_derived(&Operand::Var(VarId(3)), VarId(5)));
    }

    #[test]
    fn test_is_iv_derived_constant() {
        assert!(!is_iv_derived(&Operand::Constant(5), VarId(5)));
    }

    // ─── check_memory_safety ────────────────────────────────────

    #[test]
    fn test_check_memory_safety_different_arrays() {
        let load = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: true, data: Operand::Var(VarId(2)), dest: Some(VarId(2)),
        };
        let store = MemAccess {
            base_var: VarId(3), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(2)), dest: None,
        };
        assert!(check_memory_safety(&[load], &[store]));
    }

    #[test]
    fn test_check_memory_safety_same_array_read_write() {
        // a[i] = a[i] + 1 pattern — allowed
        let load = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: true, data: Operand::Var(VarId(2)), dest: Some(VarId(2)),
        };
        let store = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(3)), dest: None,
        };
        assert!(check_memory_safety(&[load], &[store]));
    }

    #[test]
    fn test_check_memory_safety_double_store_same_array() {
        let s1 = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(4)), dest: None,
        };
        let s2 = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(5)), dest: None,
        };
        assert!(!check_memory_safety(&[], &[s1, s2]));
    }

    #[test]
    fn test_check_memory_safety_empty() {
        assert!(check_memory_safety(&[], &[]));
    }

    #[test]
    fn test_check_memory_safety_stores_different_arrays() {
        let s1 = MemAccess {
            base_var: VarId(1), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(4)), dest: None,
        };
        let s2 = MemAccess {
            base_var: VarId(2), index_var: VarId(0), elem_type: Type::Int,
            is_load: false, data: Operand::Var(VarId(5)), dest: None,
        };
        assert!(check_memory_safety(&[], &[s1, s2]));
    }

    // ─── remap_vec_operand ──────────────────────────────────────

    #[test]
    fn test_remap_vec_operand_load_map() {
        let mut load_map = HashMap::new();
        load_map.insert(VarId(1), VarId(100));
        let op_map = HashMap::new();
        assert_eq!(
            remap_vec_operand(&Operand::Var(VarId(1)), &load_map, &op_map),
            Operand::Var(VarId(100))
        );
    }

    #[test]
    fn test_remap_vec_operand_op_map() {
        let load_map = HashMap::new();
        let mut op_map = HashMap::new();
        op_map.insert(VarId(2), VarId(200));
        assert_eq!(
            remap_vec_operand(&Operand::Var(VarId(2)), &load_map, &op_map),
            Operand::Var(VarId(200))
        );
    }

    #[test]
    fn test_remap_vec_operand_not_in_map() {
        let load_map = HashMap::new();
        let op_map = HashMap::new();
        assert_eq!(
            remap_vec_operand(&Operand::Var(VarId(99)), &load_map, &op_map),
            Operand::Var(VarId(99))
        );
    }

    #[test]
    fn test_remap_vec_operand_constant() {
        let load_map = HashMap::new();
        let op_map = HashMap::new();
        assert_eq!(
            remap_vec_operand(&Operand::Constant(42), &load_map, &op_map),
            Operand::Constant(42)
        );
    }

    #[test]
    fn test_remap_vec_operand_load_takes_priority() {
        let mut load_map = HashMap::new();
        load_map.insert(VarId(1), VarId(100));
        let mut op_map = HashMap::new();
        op_map.insert(VarId(1), VarId(200));
        // load_map takes priority via or_else
        assert_eq!(
            remap_vec_operand(&Operand::Var(VarId(1)), &load_map, &op_map),
            Operand::Var(VarId(100))
        );
    }

    // ─── find_gep_for_var ───────────────────────────────────────

    #[test]
    fn test_find_gep_for_var_found() {
        let func = make_func(vec![
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(0)),
                        element_type: Type::Int,
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let body: HashSet<BlockId> = vec![BlockId(1)].into_iter().collect();
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0));
        assert_eq!(result, Some((VarId(10), VarId(0))));
    }

    #[test]
    fn test_find_gep_for_var_wrong_iv() {
        let func = make_func(vec![
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(99)), // different IV
                        element_type: Type::Int,
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let body: HashSet<BlockId> = vec![BlockId(1)].into_iter().collect();
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0));
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_gep_for_var_not_in_body() {
        let func = make_func(vec![
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(0)),
                        element_type: Type::Int,
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let body: HashSet<BlockId> = vec![BlockId(2)].into_iter().collect(); // Block 1 not in body
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0));
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_gep_for_var_constant_base() {
        let func = make_func(vec![
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Constant(0), // Not a Var
                        index: Operand::Var(VarId(0)),
                        element_type: Type::Int,
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let body: HashSet<BlockId> = vec![BlockId(1)].into_iter().collect();
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0));
        assert_eq!(result, None);
    }

    // ─── find_max_var_id ────────────────────────────────────────

    #[test]
    fn test_find_max_var_id_basic() {
        let func = make_func(vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Binary {
                    dest: VarId(3),
                    op: BinaryOp::Add,
                    left: Operand::Constant(1),
                    right: Operand::Constant(2),
                },
                Instruction::Copy { dest: VarId(7), src: Operand::Constant(0) },
            ],
            terminator: Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 7);
    }

    #[test]
    fn test_find_max_var_id_empty() {
        let func = make_func(vec![ret_block(0)]);
        assert_eq!(find_max_var_id(&func), 0);
    }

    #[test]
    fn test_find_max_var_id_simd() {
        let func = make_func(vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Simd {
                    op: SimdOp::Load,
                    dest: Some(VarId(42)),
                    operands: vec![],
                    elem_type: Type::Int,
                    width: 8,
                },
            ],
            terminator: Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 42);
    }

    // ─── analyze_loop_body ──────────────────────────────────────

    #[test]
    fn test_analyze_loop_body_no_iv() {
        use crate::loop_analysis::NaturalLoop;

        let func = make_func(vec![ret_block(0)]);
        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(0),
            body: vec![BlockId(0)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: None,
            trip_count: Some(100),
        };

        assert!(analyze_loop_body(&func, &lp).is_none());
    }

    #[test]
    fn test_analyze_loop_body_too_small() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let func = make_func(vec![ret_block(0)]);
        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(0),
            body: vec![BlockId(0)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 3,
                cmp_op: BinaryOp::Less,
            }),
            trip_count: Some(3), // < 4
        };

        assert!(analyze_loop_body(&func, &lp).is_none());
    }

    #[test]
    fn test_analyze_loop_body_with_calls() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let func = make_func(vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Phi {
                        dest: VarId(0),
                        preds: vec![(BlockId(0), VarId(0))],
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::Call {
                        dest: None,
                        name: "printf".to_string(),
                        args: vec![],
                    },
                ],
                terminator: Terminator::Br(BlockId(0)),
                is_label_target: false,
            },
        ]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(1),
            body: vec![BlockId(0), BlockId(1)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 100,
                cmp_op: BinaryOp::Less,
            }),
            trip_count: Some(100),
        };

        assert!(analyze_loop_body(&func, &lp).is_none());
    }

    #[test]
    fn test_analyze_loop_body_complex_control_flow() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let func = make_func(vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Phi {
                        dest: VarId(0),
                        preds: vec![(BlockId(0), VarId(0))],
                    },
                ],
                terminator: Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::CondBr {
                    cond: Operand::Var(VarId(0)),
                    then_block: BlockId(2),
                    else_block: BlockId(0),
                },
                is_label_target: false,
            },
            ret_block(2),
        ]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(1),
            body: vec![BlockId(0), BlockId(1)].into_iter().collect(),
            exit: Some(BlockId(2)),
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 100,
                cmp_op: BinaryOp::Less,
            }),
            trip_count: Some(100),
        };

        // Body block 1 has CondBr → complex control flow → None
        assert!(analyze_loop_body(&func, &lp).is_none());
    }

    #[test]
    fn test_analyze_loop_body_vectorizable_copy() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        // for (i=0; i<100; i++) { b[i] = a[i]; }
        let func = make_func(vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Phi {
                        dest: VarId(0), // IV
                        preds: vec![
                            (BlockId(99), VarId(0)),
                            (BlockId(1), VarId(5)),
                        ],
                    },
                ],
                terminator: Terminator::CondBr {
                    cond: Operand::Var(VarId(0)),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
                is_label_target: false,
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(1),
                        base: Operand::Var(VarId(10)), // array a
                        index: Operand::Var(VarId(0)),
                        element_type: Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(2),
                        addr: Operand::Var(VarId(1)),
                        value_type: Type::Int,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(3),
                        base: Operand::Var(VarId(11)), // array b
                        index: Operand::Var(VarId(0)),
                        element_type: Type::Int,
                    },
                    Instruction::Store {
                        addr: Operand::Var(VarId(3)),
                        src: Operand::Var(VarId(2)),
                        value_type: Type::Int,
                    },
                    Instruction::Binary {
                        dest: VarId(5),
                        op: BinaryOp::Add,
                        left: Operand::Var(VarId(0)),
                        right: Operand::Constant(1),
                    },
                ],
                terminator: Terminator::Br(BlockId(0)),
                is_label_target: false,
            },
            ret_block(2),
        ]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(1),
            body: vec![BlockId(0), BlockId(1)].into_iter().collect(),
            exit: Some(BlockId(2)),
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 100,
                cmp_op: BinaryOp::Less,
            }),
            trip_count: Some(100),
        };

        let plan = analyze_loop_body(&func, &lp);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.loads.len(), 1);
        assert_eq!(plan.stores.len(), 1);
        assert_eq!(plan.trip_count, 100);
    }

    // ─── _type_str ──────────────────────────────────────────────

    #[test]
    fn test_type_str() {
        assert_eq!(_type_str(&Type::Float), "float");
        assert_eq!(_type_str(&Type::Double), "double");
        assert_eq!(_type_str(&Type::Int), "int32");
        assert_eq!(_type_str(&Type::Char), "int32");
    }
}
