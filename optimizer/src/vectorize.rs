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
use crate::mem_dependence::{self, check_memory_dependence};

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

/// Linear index into an array: `scale * iv + offset` (element indices).
#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexPattern {
    scale: i64,
    offset: i64,
}

impl IndexPattern {
    fn direct() -> Self {
        Self { scale: 1, offset: 0 }
    }
}

fn is_positive_power_of_two(n: i64) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

/// Resolve `op` to a linear IV index pattern using copies and arithmetic in the loop body.
fn resolve_index_pattern(
    op: &Operand,
    iv: VarId,
    func: &Function,
    body: &HashSet<BlockId>,
    arith_ops: &[(VarId, BinaryOp, Operand, Operand, bool)],
) -> Option<IndexPattern> {
    let mut visited = HashSet::new();
    resolve_index_pattern_inner(op, iv, func, body, arith_ops, &mut visited)
}

fn resolve_index_pattern_inner(
    op: &Operand,
    iv: VarId,
    func: &Function,
    body: &HashSet<BlockId>,
    arith_ops: &[(VarId, BinaryOp, Operand, Operand, bool)],
    visited: &mut HashSet<VarId>,
) -> Option<IndexPattern> {
    match op {
        Operand::Var(v) => {
            if *v == iv {
                return Some(IndexPattern::direct());
            }
            if !visited.insert(*v) {
                return None;
            }
            // Copy in loop body
            for &block_id in body {
                if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
                    for inst in &block.instructions {
                        if let Instruction::Copy { dest, src } = inst {
                            if *dest == *v {
                                return resolve_index_pattern_inner(
                                    src, iv, func, body, arith_ops, visited,
                                );
                            }
                        }
                    }
                }
            }
            // Binary defining v
            for &(dest, ref bop, ref left, ref right, _) in arith_ops {
                if dest != *v {
                    continue;
                }
                return match bop {
                    BinaryOp::Add => {
                        let lp = resolve_index_pattern_inner(left, iv, func, body, arith_ops, visited)?;
                        let rp = resolve_index_pattern_inner(right, iv, func, body, arith_ops, visited)?;
                        combine_add_patterns(lp, rp)
                    }
                    BinaryOp::Sub => {
                        let lp = resolve_index_pattern_inner(left, iv, func, body, arith_ops, visited)?;
                        let rp = resolve_index_pattern_inner(right, iv, func, body, arith_ops, visited)?;
                        combine_sub_patterns(lp, rp)
                    }
                    BinaryOp::Mul => {
                        let lp = resolve_index_pattern_inner(left, iv, func, body, arith_ops, visited)?;
                        let rp = resolve_index_pattern_inner(right, iv, func, body, arith_ops, visited)?;
                        combine_mul_patterns(lp, rp)
                    }
                    _ => None,
                };
            }
            None
        }
        Operand::Constant(c) => {
            if *c == 0 {
                Some(IndexPattern { scale: 0, offset: 0 })
            } else {
                Some(IndexPattern { scale: 0, offset: *c })
            }
        }
        _ => None,
    }
}

fn combine_add_patterns(a: IndexPattern, b: IndexPattern) -> Option<IndexPattern> {
    if a.scale == 0 && b.scale == 0 {
        return Some(IndexPattern { scale: 0, offset: a.offset + b.offset });
    }
    if a.scale == 0 {
        return Some(IndexPattern { scale: b.scale, offset: a.offset + b.offset });
    }
    if b.scale == 0 {
        return Some(IndexPattern { scale: a.scale, offset: a.offset + b.offset });
    }
    if a.scale == b.scale {
        return Some(IndexPattern { scale: a.scale, offset: a.offset + b.offset });
    }
    None
}

fn combine_sub_patterns(a: IndexPattern, b: IndexPattern) -> Option<IndexPattern> {
    if b.scale == 0 {
        return Some(IndexPattern { scale: a.scale, offset: a.offset - b.offset });
    }
    if a.scale == 0 && b.scale != 0 {
        return Some(IndexPattern { scale: -b.scale, offset: a.offset - b.offset });
    }
    None
}

fn combine_mul_patterns(a: IndexPattern, b: IndexPattern) -> Option<IndexPattern> {
    match (a.scale, b.scale) {
        (0, 0) => {
            let product = a.offset.wrapping_mul(b.offset);
            if is_positive_power_of_two(product) || product == 0 {
                Some(IndexPattern { scale: 0, offset: product })
            } else {
                None
            }
        }
        (0, s) | (s, 0) if s != 0 => {
            let c = if a.scale == 0 { a.offset } else { b.offset };
            if is_positive_power_of_two(c) {
                Some(IndexPattern { scale: s * c, offset: 0 })
            } else {
                None
            }
        }
        (sa, sb) if sa != 0 && sb != 0 => None,
        _ => None,
    }
}

/// Emit instructions computing `pattern.scale * vec_iv + pattern.offset` into `insts`.
/// Returns the VarId holding the GEP index.
fn emit_scaled_index(
    insts: &mut Vec<Instruction>,
    vec_iv: VarId,
    pattern: &IndexPattern,
    next_var: &mut usize,
) -> VarId {
    if pattern.scale == 1 && pattern.offset == 0 {
        return vec_iv;
    }
    let mut idx = vec_iv;
    if pattern.scale != 1 {
        let mul_dest = VarId(*next_var);
        *next_var += 1;
        insts.push(Instruction::Binary {
            dest: mul_dest,
            op: BinaryOp::Mul,
            left: Operand::Var(vec_iv),
            right: Operand::Constant(pattern.scale),
        });
        idx = mul_dest;
    }
    if pattern.offset != 0 {
        let add_dest = VarId(*next_var);
        *next_var += 1;
        insts.push(Instruction::Binary {
            dest: add_dest,
            op: BinaryOp::Add,
            left: Operand::Var(idx),
            right: Operand::Constant(pattern.offset),
        });
        idx = add_dest;
    }
    idx
}

/// Information about a vectorizable memory access in a loop
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MemAccess {
    /// The IR variable holding the array base address
    base_var: VarId,
    /// Linear index: scale * IV + offset
    index_pattern: IndexPattern,
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
    let bound_operand = iv.bound_operand.clone();
    let dynamic_bound = !matches!(bound_operand, Operand::Constant(_));

    let trip_count = if dynamic_bound {
        // `for (i = init; i < bound_var; i += step)` with runtime trip count
        if iv.init != 0 || iv.step != 1 {
            return None;
        }
        if !matches!(iv.cmp_op, BinaryOp::GreaterEqual) {
            return None;
        }
        None
    } else {
        let tc = lp.trip_count?;
        if tc < 4 {
            return None;
        }
        Some(tc)
    };

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

    // Pass 1: collect arithmetic (needed to resolve linear index patterns)
    for &block_id in &lp.body {
        let block = func.blocks.iter().find(|b| b.id == block_id)?;
        for inst in &block.instructions {
            match inst {
                Instruction::Call { .. } | Instruction::IndirectCall { .. } => has_calls = true,
                Instruction::Binary { dest, op, left, right } => {
                    arithmetic_ops.push((*dest, op.clone(), left.clone(), right.clone(), false));
                }
                Instruction::FloatBinary { dest, op, left, right } => {
                    arithmetic_ops.push((*dest, op.clone(), left.clone(), right.clone(), true));
                }
                _ => {}
            }
        }
    }

    // Pass 2: memory accesses with linear IV index patterns
    for &block_id in &lp.body {
        let block = func.blocks.iter().find(|b| b.id == block_id)?;
        for inst in &block.instructions {
            match inst {
                Instruction::Load { dest, addr, value_type, .. } => {
                    if let Operand::Var(addr_var) = addr {
                        if let Some((base, pat)) = find_gep_for_var(
                            func, *addr_var, &lp.body, iv.var, &arithmetic_ops,
                        ) {
                            if pat.scale > 0 && is_positive_power_of_two(pat.scale) {
                                loads.push(MemAccess {
                                    base_var: base,
                                    index_pattern: pat,
                                    elem_type: value_type.clone(),
                                    is_load: true,
                                    data: Operand::Var(*dest),
                                    dest: Some(*dest),
                                });
                            }
                        }
                    }
                }
                Instruction::Store { addr, src, value_type, .. } => {
                    if let Operand::Var(addr_var) = addr {
                        if let Some((base, pat)) = find_gep_for_var(
                            func, *addr_var, &lp.body, iv.var, &arithmetic_ops,
                        ) {
                            if pat.scale > 0 && is_positive_power_of_two(pat.scale) {
                                stores.push(MemAccess {
                                    base_var: base,
                                    index_pattern: pat,
                                    elem_type: value_type.clone(),
                                    is_load: false,
                                    data: src.clone(),
                                    dest: None,
                                });
                            }
                        }
                    }
                }
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
        bound_operand,
        dynamic_bound,
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

/// Find the GEP instruction that produces a given variable, returning (base_var, index pattern).
fn find_gep_for_var(
    func: &Function,
    var: VarId,
    body: &HashSet<BlockId>,
    iv_var: VarId,
    arith_ops: &[(VarId, BinaryOp, Operand, Operand, bool)],
) -> Option<(VarId, IndexPattern)> {
    for &block_id in body {
        if let Some(block) = func.blocks.iter().find(|b| b.id == block_id) {
            for inst in &block.instructions {
                if let Instruction::GetElementPtr { dest, base, index, .. } = inst {
                    if *dest == var {
                        if let Operand::Var(base_v) = base {
                            if let Some(pat) =
                                resolve_index_pattern(index, iv_var, func, body, arith_ops)
                            {
                                if pat.scale > 0 {
                                    return Some((*base_v, pat));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Plan for vectorizing a loop
#[derive(Debug)]
struct VectorizationPlan {
    trip_count: Option<usize>,
    bound_operand: Operand,
    dynamic_bound: bool,
    loads: Vec<MemAccess>,
    stores: Vec<MemAccess>,
    reductions: Vec<Reduction>,
    arithmetic_ops: Vec<(VarId, BinaryOp, Operand, Operand, bool)>,
}

/// Rough profitability: vectorization must amortize peel/setup cost.
fn is_vectorization_profitable(plan: &VectorizationPlan, vf: usize) -> bool {
    let mem_ops = plan.loads.len() + plan.stores.len();
    if mem_ops == 0 {
        return false;
    }

    // Pure store-fill (invariant splat) with no loads/reductions is never worth it.
    if plan.loads.is_empty() && plan.reductions.is_empty() {
        return false;
    }

    let vector_mem_work = mem_ops.saturating_mul(vf);

    match plan.trip_count {
        Some(tc) => {
            if tc < vf {
                return false;
            }
            // Need either multiple vector iterations or enough per-iter memory work.
            tc >= vf * 2 || vector_mem_work >= vf + 4
        }
        None => {
            // Runtime-bound loops: require substantive memory traffic per iteration.
            plan.dynamic_bound && vector_mem_work >= vf
        }
    }
}

/// Main auto-vectorization entry point
/// Analyzes loops and inserts vector IR annotations that codegen will use
pub fn vectorize_function(func: &mut Function, simd_level: SimdLevel) {
    let loops = loop_analysis::find_loops(func);
    let vf = simd_level.vector_width();

    for lp in &loops {
        if let Some(plan) = analyze_loop_body(func, lp) {
            if !memory_dependence_ok(func, &plan, vf) {
                continue;
            }
            if is_vectorization_profitable(&plan, vf) {
                apply_vectorization(func, lp, &plan, simd_level);
            }
        }
    }
}

fn memory_dependence_ok(func: &Function, plan: &VectorizationPlan, vf: usize) -> bool {
    let dep_loads: Vec<mem_dependence::MemAccess> = plan
        .loads
        .iter()
        .map(convert_mem_access)
        .collect();
    let dep_stores: Vec<mem_dependence::MemAccess> = plan
        .stores
        .iter()
        .map(convert_mem_access)
        .collect();
    let arith_ops: Vec<(VarId, Operand, Operand)> = plan
        .arithmetic_ops
        .iter()
        .map(|(d, _, l, r, _)| (*d, l.clone(), r.clone()))
        .collect();
    check_memory_dependence(func, &dep_loads, &dep_stores, vf, &arith_ops)
}

fn convert_mem_access(m: &MemAccess) -> mem_dependence::MemAccess {
    mem_dependence::MemAccess {
        base_var: m.base_var,
        index_pattern: mem_dependence::IndexPattern {
            scale: m.index_pattern.scale,
            offset: m.index_pattern.offset,
        },
        is_load: m.is_load,
        data: m.data.clone(),
        dest: m.dest,
    }
}

/// Append vector loads/stores/arithmetic for one vectorized iteration.
/// When `mask_var` is set, inactive lanes are zeroed on loads and merged on stores (tail epilogue).
fn append_vec_loop_body(
    insts: &mut Vec<Instruction>,
    plan: &VectorizationPlan,
    vec_iv: VarId,
    iv_var: VarId,
    vf: usize,
    mask_var: Option<VarId>,
    next_var: &mut usize,
    load_vec_vars: &mut HashMap<VarId, VarId>,
    op_vec_vars: &mut HashMap<VarId, VarId>,
    reduction_infos: &[ReductionInfo],
) -> usize {
    let reduction_accums: HashSet<VarId> = reduction_infos.iter().map(|r| r.accum_var).collect();
    let mut valid_simd_stores = 0;

    for load in &plan.loads {
        let gep_dest = VarId(*next_var);
        *next_var += 1;
        let vec_load_dest = VarId(*next_var);
        *next_var += 1;

        let index_var = emit_scaled_index(insts, vec_iv, &load.index_pattern, next_var);
        insts.push(Instruction::GetElementPtr {
            dest: gep_dest,
            base: Operand::Var(load.base_var),
            index: Operand::Var(index_var),
            element_type: load.elem_type.clone(),
        });
        insts.push(Instruction::Simd {
            op: SimdOp::Load,
            dest: Some(vec_load_dest),
            operands: vec![Operand::Var(gep_dest)],
            elem_type: load.elem_type.clone(),
            width: vf,
        });

        let mut vec_reg = vec_load_dest;
        if let Some(mask) = mask_var {
            let masked = VarId(*next_var);
            *next_var += 1;
            insts.push(Instruction::Simd {
                op: SimdOp::And,
                dest: Some(masked),
                operands: vec![Operand::Var(vec_reg), Operand::Var(mask)],
                elem_type: load.elem_type.clone(),
                width: vf,
            });
            vec_reg = masked;
        }
        if let Some(orig_dest) = load.dest {
            load_vec_vars.insert(orig_dest, vec_reg);
        }
    }

    for &(dest, ref op, ref left, ref right, is_float) in &plan.arithmetic_ops {
        if dest == iv_var {
            continue;
        }
        let left_is_accum = matches!(left, Operand::Var(v) if reduction_accums.contains(v));
        let right_is_accum = matches!(right, Operand::Var(v) if reduction_accums.contains(v));
        if left_is_accum || right_is_accum {
            let accum_var = if left_is_accum {
                match left { Operand::Var(v) => *v, _ => continue }
            } else {
                match right { Operand::Var(v) => *v, _ => continue }
            };
            let other_operand = if left_is_accum { right } else { left };
            if let Some(rinfo) = reduction_infos.iter().find(|r| r.accum_var == accum_var) {
                let vec_other = remap_vec_operand(other_operand, load_vec_vars, op_vec_vars);
                let is_remapped = matches!(
                    (other_operand, &vec_other),
                    (Operand::Var(orig), Operand::Var(mapped)) if orig != mapped
                );
                if is_remapped {
                    let simd_op = match op {
                        BinaryOp::Add => SimdOp::Add,
                        BinaryOp::Mul => SimdOp::Mul,
                        _ => continue,
                    };
                    let elem_type = if is_float { Type::Float } else { Type::Int };
                    insts.push(Instruction::Simd {
                        op: simd_op,
                        dest: Some(rinfo.vec_accum),
                        operands: vec![Operand::Var(rinfo.vec_accum), vec_other],
                        elem_type,
                        width: vf,
                    });
                    op_vec_vars.insert(dest, rinfo.vec_accum);
                }
            }
            continue;
        }

        let left_is_vec = matches!(left, Operand::Var(v) if load_vec_vars.contains_key(v) || op_vec_vars.contains_key(v));
        let right_is_vec = matches!(right, Operand::Var(v) if load_vec_vars.contains_key(v) || op_vec_vars.contains_key(v));
        if !left_is_vec && !right_is_vec {
            continue;
        }

        let simd_op = match op {
            BinaryOp::Add => SimdOp::Add,
            BinaryOp::Sub => SimdOp::Sub,
            BinaryOp::Mul => SimdOp::Mul,
            BinaryOp::BitwiseAnd => SimdOp::And,
            BinaryOp::BitwiseOr => SimdOp::Or,
            BinaryOp::BitwiseXor => SimdOp::Xor,
            _ => continue,
        };
        let vec_dest = VarId(*next_var);
        *next_var += 1;
        let op_elem_type = if is_float { Type::Float } else { Type::Int };
        insts.push(Instruction::Simd {
            op: simd_op,
            dest: Some(vec_dest),
            operands: vec![
                remap_vec_operand(left, load_vec_vars, op_vec_vars),
                remap_vec_operand(right, load_vec_vars, op_vec_vars),
            ],
            elem_type: op_elem_type,
            width: vf,
        });
        op_vec_vars.insert(dest, vec_dest);
    }

    for store in &plan.stores {
        let vec_src = remap_vec_operand(&store.data, load_vec_vars, op_vec_vars);
        let is_remapped = matches!(
            (&store.data, &vec_src),
            (Operand::Var(orig), Operand::Var(mapped)) if orig != mapped
        );
        let final_vec_src = if !is_remapped {
            let splat_dest = VarId(*next_var);
            *next_var += 1;
            insts.push(Instruction::Simd {
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

        let gep_dest = VarId(*next_var);
        *next_var += 1;
        let index_var = emit_scaled_index(insts, vec_iv, &store.index_pattern, next_var);
        insts.push(Instruction::GetElementPtr {
            dest: gep_dest,
            base: Operand::Var(store.base_var),
            index: Operand::Var(index_var),
            element_type: store.elem_type.clone(),
        });

        let store_src = if let Some(mask) = mask_var {
            let mem_vec = VarId(*next_var);
            *next_var += 1;
            insts.push(Instruction::Simd {
                op: SimdOp::Load,
                dest: Some(mem_vec),
                operands: vec![Operand::Var(gep_dest)],
                elem_type: store.elem_type.clone(),
                width: vf,
            });
            let blended = VarId(*next_var);
            *next_var += 1;
            insts.push(Instruction::Simd {
                op: SimdOp::Blend,
                dest: Some(blended),
                operands: vec![Operand::Var(mem_vec), final_vec_src, Operand::Var(mask)],
                elem_type: store.elem_type.clone(),
                width: vf,
            });
            Operand::Var(blended)
        } else {
            final_vec_src
        };

        insts.push(Instruction::Simd {
            op: SimdOp::Store,
            dest: None,
            operands: vec![Operand::Var(gep_dest), store_src],
            elem_type: store.elem_type.clone(),
            width: vf,
        });
        valid_simd_stores += 1;
    }

    valid_simd_stores
}

struct ReductionInfo {
    accum_var: VarId,
    vec_accum: VarId,
    scalar_result: VarId,
    op: BinaryOp,
    is_float: bool,
    elem_type: Type,
}

/// Apply vectorization to a loop by transforming it into a vectorized + remainder structure.
///
/// Original: for (i = 0; i < N; i++) body(i)
/// Becomes:  for (i = 0; i < N - N%VF; i += VF) vector_body(i)
///           [masked vector tail] or scalar remainder loop for reductions
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

    // Must have array memory accesses to vectorize
    if plan.loads.is_empty() && plan.stores.is_empty() {
        return;
    }

    let (vec_trip_count, has_remainder, vector_limit_operand) = if plan.dynamic_bound {
        // Filled in preheader: limit = bound - (bound % vf) for init=0, step=1
        (0usize, true, None)
    } else {
        let trip_count = match plan.trip_count {
            Some(tc) => tc,
            None => return,
        };
        let vec_iters = trip_count / vf;
        if vec_iters == 0 {
            return;
        }
        let vec_trip_count = vec_iters * vf;
        let has_remainder = trip_count % vf != 0;
        (
            vec_trip_count,
            has_remainder,
            Some(Operand::Constant(vec_trip_count as i64)),
        )
    };

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
    let vec_limit_var = if plan.dynamic_bound {
        Some(VarId(max_var_id + 5))
    } else {
        None
    };
    let mut next_var = max_var_id + if plan.dynamic_bound { 6 } else { 5 };

    let use_masked_tail = has_remainder && plan.reductions.is_empty();
    let tail_elem_type = if !plan.loads.is_empty() {
        plan.loads[0].elem_type.clone()
    } else {
        plan.stores[0].elem_type.clone()
    };

    // --- Build vectorized body instructions ---
    let mut vec_body_insts: Vec<Instruction> = Vec::new();
    let mut load_vec_vars: HashMap<VarId, VarId> = HashMap::new();
    let mut op_vec_vars: HashMap<VarId, VarId> = HashMap::new();

    let mut reduction_infos: Vec<ReductionInfo> = Vec::new();
    for red in &plan.reductions {
        let vec_accum = VarId(next_var); next_var += 1;
        let scalar_result = VarId(next_var); next_var += 1;
        let elem_type = if red.is_float { Type::Float } else { Type::Int };
        reduction_infos.push(ReductionInfo {
            accum_var: red.accum_var,
            vec_accum,
            scalar_result,
            op: red.op.clone(),
            is_float: red.is_float,
            elem_type,
        });
    }

    let valid_simd_stores = append_vec_loop_body(
        &mut vec_body_insts,
        plan,
        vec_iv,
        iv.var,
        vf,
        None,
        &mut next_var,
        &mut load_vec_vars,
        &mut op_vec_vars,
        &reduction_infos,
    );

    // Bail out if no useful SIMD work was generated.
    // We need at least one SIMD store OR one active reduction to justify vectorization.
    let active_reductions: Vec<&ReductionInfo> = reduction_infos.iter()
        .filter(|r| {
            // A reduction is "active" if vec_accum was used as dest in a SIMD op
            vec_body_insts.iter().any(|inst| {
                matches!(inst, Instruction::Simd { dest: Some(d), .. } if *d == r.vec_accum)
            })
        })
        .collect();
    if valid_simd_stores == 0 && active_reductions.is_empty() {
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
    // Reduction accumulators use in-place SIMD register updates (no phi needed)
    let vec_header_insts = vec![
        // Phi: vec_iv comes from preheader (init) or vec_body (vec_iv_next)
        Instruction::Phi {
            dest: vec_iv,
            preds: vec![
                (preheader, vec_init_iv),
                (vec_body_id, vec_iv_next),
            ],
        },
        // Compare: vec_iv < limit (constant peeled count or runtime limit)
        Instruction::Binary {
            dest: vec_cmp,
            op: BinaryOp::Less,
            left: Operand::Var(vec_iv),
            right: vector_limit_operand.clone().unwrap_or_else(|| {
                Operand::Var(vec_limit_var.expect("dynamic bound requires vec_limit_var"))
            }),
        },
    ];

    // Optional masked tail block (one partial vector iteration).
    let mut next_extra_block = max_block_id + 3;
    let tail_block_id = if use_masked_tail {
        let id = BlockId(next_extra_block);
        next_extra_block += 1;
        let mask_var = VarId(next_var);
        next_var += 1;
        let bound_op = if plan.dynamic_bound {
            plan.bound_operand.clone()
        } else {
            Operand::Constant(iv.bound)
        };
        let mut tail_insts = vec![Instruction::Simd {
            op: SimdOp::LaneMask,
            dest: Some(mask_var),
            operands: vec![Operand::Var(vec_iv), bound_op],
            elem_type: tail_elem_type.clone(),
            width: vf,
        }];
        let mut tail_load_map = HashMap::new();
        let mut tail_op_map = HashMap::new();
        append_vec_loop_body(
            &mut tail_insts,
            plan,
            vec_iv,
            iv.var,
            vf,
            Some(mask_var),
            &mut next_var,
            &mut tail_load_map,
            &mut tail_op_map,
            &[],
        );
        func.blocks.push(BasicBlock {
            id,
            instructions: tail_insts,
            terminator: Terminator::Br(exit_block),
            is_label_target: false,
        });
        Some(id)
    } else {
        None
    };

    // Bridge block for reduction horizontal adds.
    let vec_exit_target = if !active_reductions.is_empty() {
        let bridge_id = BlockId(next_extra_block);
        let mut bridge_insts: Vec<Instruction> = Vec::new();

        for rinfo in &active_reductions {
            bridge_insts.push(Instruction::Simd {
                op: SimdOp::HorizontalAdd,
                dest: Some(rinfo.scalar_result),
                operands: vec![Operand::Var(rinfo.vec_accum)],
                elem_type: rinfo.elem_type.clone(),
                width: vf,
            });
        }

        let bridge_target = if use_masked_tail {
            tail_block_id.expect("masked tail block")
        } else if has_remainder {
            lp.header
        } else {
            exit_block
        };
        func.blocks.push(BasicBlock {
            id: bridge_id,
            instructions: bridge_insts,
            terminator: Terminator::Br(bridge_target),
            is_label_target: false,
        });
        bridge_id
    } else if use_masked_tail {
        tail_block_id.expect("masked tail block")
    } else if has_remainder {
        lp.header
    } else {
        exit_block
    };

    let vec_header = BasicBlock {
        id: vec_header_id,
        instructions: vec_header_insts,
        terminator: Terminator::CondBr {
            cond: Operand::Var(vec_cmp),
            then_block: vec_body_id,
            else_block: vec_exit_target,
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

        if plan.dynamic_bound {
            let limit_var = vec_limit_var.expect("vec_limit_var");
            let rem_var = VarId(next_var);
            next_var += 1;
            // limit = bound - (bound % vf)  (init=0, step=1)
            pre_block.instructions.push(Instruction::Binary {
                dest: rem_var,
                op: BinaryOp::Mod,
                left: plan.bound_operand.clone(),
                right: Operand::Constant(vf as i64),
            });
            pre_block.instructions.push(Instruction::Binary {
                dest: limit_var,
                op: BinaryOp::Sub,
                left: plan.bound_operand.clone(),
                right: Operand::Var(rem_var),
            });
        }

        // Add zero-vector init for each active reduction (directly to vec_accum)
        for rinfo in &active_reductions {
            // Initialize vector accumulator to zero (identity for addition)
            pre_block.instructions.push(Instruction::Simd {
                op: SimdOp::Splat,
                dest: Some(rinfo.vec_accum),
                operands: vec![Operand::Constant(0)],
                elem_type: rinfo.elem_type.clone(),
                width: vf,
            });
        }

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

    // --- Handle remainder: update original loop's IV and reduction phi nodes ---
    // Determine which block feeds into the original loop header from the vec loop exit
    let vec_exit_feeding_block = if !active_reductions.is_empty() {
        // The bridge block feeds into the original header
        vec_exit_target
    } else {
        vec_header_id
    };

    if has_remainder && !use_masked_tail {
        // Scalar remainder loop (reductions or no masked-tail support).
        // After vectorization, when we fall through from vec loop → original header,
        // the coming-from block is vec_exit_feeding_block (not preheader anymore).
        if let Some(header_block) = func.blocks.iter_mut().find(|b| b.id == lp.header) {
            for inst in &mut header_block.instructions {
                if let Instruction::Phi { dest, preds } = inst {
                    if *dest == iv.var {
                        // Change the preheader source to come from vec exit block instead
                        for (pred_block, _pred_var) in preds.iter_mut() {
                            if *pred_block == preheader {
                                *pred_block = vec_exit_feeding_block;
                            }
                        }
                    }
                }
            }
        }
        // Update the phi to use vec_iv from vec_header_id (the IV at vec loop exit)
        if let Some(header_block) = func.blocks.iter_mut().find(|b| b.id == lp.header) {
            for inst in &mut header_block.instructions {
                if let Instruction::Phi { dest, preds } = inst {
                    if *dest == iv.var {
                        for (pred_block, pred_var) in preds.iter_mut() {
                            if *pred_block == vec_exit_feeding_block {
                                *pred_var = vec_iv;
                            }
                        }
                    }
                    // Update reduction accumulator phis in the original header
                    for rinfo in &active_reductions {
                        if *dest == rinfo.accum_var {
                            // The accumulator's init now comes from the bridge block's
                            // HorizontalAdd scalar result instead of the preheader
                            for (pred_block, pred_var) in preds.iter_mut() {
                                if *pred_block == preheader {
                                    *pred_block = vec_exit_feeding_block;
                                    *pred_var = rinfo.scalar_result;
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // No remainder: vec loop exits directly to exit_block (via bridge if reductions)
        // If we have reductions, we need to patch the exit_block to use scalar_result
        // where the original accumulator was used.
        if !active_reductions.is_empty() {
            // The original loop is unreachable, but the exit block may reference
            // the accumulator variable. We need to add copies so the scalar_result
            // feeds into whatever uses the accumulator in the exit block.
            // Actually, in the no-remainder case, the vec loop processes ALL iterations.
            // The exit block's references to the accumulator will be handled by
            // the phi resolution — the accumulator's final value comes from the
            // original loop's latch. Since the original loop is now unreachable,
            // we need to make the exit block use the scalar_result.
            // Simplest approach: add Copy instructions at the start of the bridge block.
            if let Some(bridge) = func.blocks.iter_mut().find(|b| b.id == vec_exit_target) {
                for rinfo in &active_reductions {
                    bridge.instructions.push(Instruction::Copy {
                        dest: rinfo.accum_var,
                        src: Operand::Var(rinfo.scalar_result),
                    });
                }
            }
        }
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
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0), &[]);
        assert_eq!(result, Some((VarId(10), IndexPattern::direct())));
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
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0), &[]);
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
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0), &[]);
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
        let result = find_gep_for_var(&func, VarId(5), &body, VarId(0), &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_index_pattern_iv_plus_const() {
        let body: HashSet<BlockId> = [BlockId(1)].into_iter().collect();
        let func = make_func(vec![BasicBlock {
            id: BlockId(1),
            instructions: vec![Instruction::Binary {
                dest: VarId(7),
                op: BinaryOp::Add,
                left: Operand::Var(VarId(0)),
                right: Operand::Constant(2),
            }],
            terminator: Terminator::Br(BlockId(1)),
            is_label_target: false,
        }]);
        let arith = vec![(VarId(7), BinaryOp::Add, Operand::Var(VarId(0)), Operand::Constant(2), false)];
        let pat = resolve_index_pattern(&Operand::Var(VarId(7)), VarId(0), &func, &body, &arith);
        assert_eq!(pat, Some(IndexPattern { scale: 1, offset: 2 }));
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
                bound_operand: Operand::Constant(3),
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
                bound_operand: Operand::Constant(100),
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
                bound_operand: Operand::Constant(100),
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
                        volatile: false,
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
                        volatile: false,
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
                bound_operand: Operand::Constant(100),
                cmp_op: BinaryOp::Less,
            }),
            trip_count: Some(100),
        };

        let plan = analyze_loop_body(&func, &lp);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.loads.len(), 1);
        assert_eq!(plan.stores.len(), 1);
        assert_eq!(plan.trip_count, Some(100));
        assert!(!plan.dynamic_bound);
    }

    // ─── _type_str ──────────────────────────────────────────────

    #[test]
    fn test_vectorize_maps_bitwise_and() {
        let map_op = |op: BinaryOp| match op {
            BinaryOp::BitwiseAnd => SimdOp::And,
            BinaryOp::BitwiseOr => SimdOp::Or,
            BinaryOp::BitwiseXor => SimdOp::Xor,
            _ => SimdOp::Add,
        };
        assert_eq!(map_op(BinaryOp::BitwiseAnd), SimdOp::And);
    }

    #[test]
    fn test_profitable_vector_loop() {
        let plan = VectorizationPlan {
            trip_count: Some(100),
            bound_operand: Operand::Constant(100),
            dynamic_bound: false,
            loads: vec![MemAccess {
                base_var: VarId(1),
                index_pattern: IndexPattern::direct(),
                elem_type: Type::Int,
                is_load: true,
                data: Operand::Var(VarId(2)),
                dest: Some(VarId(2)),
            }],
            stores: vec![],
            reductions: vec![],
            arithmetic_ops: vec![],
        };
        assert!(is_vectorization_profitable(&plan, 4));
    }

    #[test]
    fn test_unprofitable_tiny_trip() {
        let plan = VectorizationPlan {
            trip_count: Some(3),
            bound_operand: Operand::Constant(3),
            dynamic_bound: false,
            loads: vec![MemAccess {
                base_var: VarId(1),
                index_pattern: IndexPattern::direct(),
                elem_type: Type::Int,
                is_load: true,
                data: Operand::Var(VarId(2)),
                dest: Some(VarId(2)),
            }],
            stores: vec![],
            reductions: vec![],
            arithmetic_ops: vec![],
        };
        assert!(!is_vectorization_profitable(&plan, 4));
    }

    #[test]
    fn test_type_str() {
        assert_eq!(_type_str(&Type::Float), "float");
        assert_eq!(_type_str(&Type::Double), "double");
        assert_eq!(_type_str(&Type::Int), "int32");
        assert_eq!(_type_str(&Type::Char), "int32");
    }
}
