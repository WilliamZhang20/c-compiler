// Memory dependence and alias checks for loop vectorization.
//
// Verifies that widening the induction variable by VF (SIMD width) does not
// introduce loop-carried or cross-lane dependencies on the same underlying object.

use ir::{Function, Instruction, Operand, VarId};
use std::collections::HashSet;

/// Linear element index: scale * IV + offset (IV advances by 1 each scalar iteration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPattern {
    pub scale: i64,
    pub offset: i64,
}

impl IndexPattern {
    pub fn direct() -> Self {
        Self { scale: 1, offset: 0 }
    }
}

/// One memory operation in a loop body relevant to vectorization.
#[derive(Debug, Clone)]
pub struct MemAccess {
    pub base_var: VarId,
    pub index_pattern: IndexPattern,
    pub is_load: bool,
    /// Load dest or store source.
    pub data: Operand,
    pub dest: Option<VarId>,
}

/// Element-index span touched by one SIMD access when the vector IV starts at `v`.
/// Indices are `scale * (v + k) + offset` for k in 0..vf-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndexSpan {
    min: i64,
    max: i64,
}

impl IndexPattern {
    /// Span of element indices in one VF-wide SIMD access (IV step must be 1).
    fn simd_span(&self, vf: usize) -> Option<IndexSpan> {
        if self.scale <= 0 || vf == 0 {
            return None;
        }
        // Packed load/store reads `vf` consecutive elements starting at GEP index
        // (scale * v + offset). That matches scalar indices scale*(v+k)+offset only
        // when scale == 1.
        if self.scale != 1 {
            return None;
        }
        let vf = vf as i64;
        Some(IndexSpan {
            min: self.offset,
            max: self.offset + vf - 1,
        })
    }
}

fn spans_overlap(a: IndexSpan, b: IndexSpan) -> bool {
    a.min <= b.max && b.min <= a.max
}

/// Whether `var` is derived from `source` through copies and binary ops in the loop body.
fn var_derived_from(
    var: VarId,
    source: VarId,
    arith_ops: &[(VarId, Operand, Operand)],
) -> bool {
    if var == source {
        return true;
    }
    let mut visited = HashSet::new();
    let mut work = vec![var];
    while let Some(v) = work.pop() {
        if v == source {
            return true;
        }
        if !visited.insert(v) {
            continue;
        }
        for &(dest, ref left, ref right) in arith_ops {
            if dest != v {
                continue;
            }
            if let Operand::Var(lv) = left {
                work.push(*lv);
            }
            if let Operand::Var(rv) = right {
                work.push(*rv);
            }
        }
    }
    false
}

/// Store `src` is derived from the value loaded at `load` (possibly via arithmetic).
fn store_uses_loaded_value(
    src: &Operand,
    load_dest: VarId,
    load_dests: &HashSet<VarId>,
    arith_ops: &[(VarId, Operand, Operand)],
) -> bool {
    match src {
        Operand::Var(v) => {
            *v == load_dest
                || load_dests.contains(v)
                || var_derived_from(*v, load_dest, arith_ops)
        }
        _ => false,
    }
}

/// Intra-vector and inter-chunk dependence check (same underlying object).
fn check_same_base_dependences(
    loads: &[MemAccess],
    stores: &[MemAccess],
    vf: usize,
    arith_ops: &[(VarId, Operand, Operand)],
) -> bool {
    for access in loads.iter().chain(stores.iter()) {
        if access.index_pattern.simd_span(vf).is_none() {
            return false;
        }
    }

    // Duplicate store to identical element slot (same base, scale, offset).
    let mut store_slots = HashSet::new();
    for store in stores {
        let key = (
            store.base_var,
            store.index_pattern.scale,
            store.index_pattern.offset,
        );
        if !store_slots.insert(key) {
            return false;
        }
    }

    let load_dests: HashSet<VarId> = loads.iter().filter_map(|l| l.dest).collect();

    // Pairwise overlap on the same base.
    let all: Vec<&MemAccess> = loads.iter().chain(stores.iter()).collect();
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            let a = all[i];
            let b = all[j];
            if a.base_var != b.base_var {
                continue;
            }
            let Some(sa) = a.index_pattern.simd_span(vf) else {
                return false;
            };
            let Some(sb) = b.index_pattern.simd_span(vf) else {
                return false;
            };
            if !spans_overlap(sa, sb) {
                continue;
            }

            // Two stores to overlapping indices — unsafe.
            if !a.is_load && !b.is_load {
                return false;
            }

            // Store + load overlap: RAW unless store reads exactly this load's value
            // (element-wise copy / in-place update from same indices).
            if !a.is_load && b.is_load {
                if let Some(ld) = b.dest {
                    if !store_uses_loaded_value(&a.data, ld, &load_dests, arith_ops) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if a.is_load && !b.is_load {
                if let Some(ld) = a.dest {
                    if !store_uses_loaded_value(&b.data, ld, &load_dests, arith_ops) {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // load + load overlap is fine (read-only).
        }
    }

    // Inter-vector-chunk dependence per base (store chunk before load chunk).
    let mut bases = HashSet::new();
    for a in loads.iter().chain(stores.iter()) {
        bases.insert(a.base_var);
    }
    for base in bases {
        let base_stores: Vec<_> = stores.iter().filter(|s| s.base_var == base).collect();
        let base_loads: Vec<_> = loads.iter().filter(|l| l.base_var == base).collect();
        if base_stores.is_empty() || base_loads.is_empty() {
            continue;
        }
        // Chunk k ends at index (k+1)*VF - 1 + max(store.offset); chunk k+1 loads from
        // (k+1)*VF + min(load.offset). Equivalently at k=0: store.offset + VF - 1 < VF + load.offset.
        let vf_i = vf as i64;
        let max_store_end = base_stores
            .iter()
            .map(|s| s.index_pattern.offset + vf_i - 1)
            .max()
            .unwrap_or(i64::MIN);
        let min_load_next_chunk = base_loads
            .iter()
            .map(|l| l.index_pattern.offset + vf_i)
            .min()
            .unwrap_or(i64::MAX);
        if max_store_end >= min_load_next_chunk {
            return false;
        }
    }

    true
}

/// Root allocation for alias partitioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PointerRoot {
    Alloca(VarId),
    Global,
    Unknown,
}

fn trace_pointer_root(func: &Function, var: VarId) -> PointerRoot {
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Alloca { dest, .. } = inst {
                if *dest == var {
                    return PointerRoot::Alloca(var);
                }
            }
        }
    }

    let mut visited = HashSet::new();
    let mut work = vec![var];
    while let Some(v) = work.pop() {
        if !visited.insert(v) {
            continue;
        }
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    Instruction::Alloca { dest, .. } if *dest == v => {
                        return PointerRoot::Alloca(v);
                    }
                    Instruction::Copy { dest, src } if *dest == v => {
                        if let Operand::Var(sv) = src {
                            work.push(*sv);
                        } else if let Operand::Global(_) = src {
                            return PointerRoot::Global;
                        }
                    }
                    Instruction::GetElementPtr { dest, base, .. } if *dest == v => {
                        if let Operand::Var(b) = base {
                            work.push(*b);
                        } else if let Operand::Global(_) = base {
                            return PointerRoot::Global;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    PointerRoot::Unknown
}

/// Different bases must not resolve to the same unknown/possibly-aliasing root.
fn check_distinct_base_aliases(func: &Function, loads: &[MemAccess], stores: &[MemAccess]) -> bool {
    let mut roots = HashSet::new();
    for access in loads.iter().chain(stores.iter()) {
        roots.insert(trace_pointer_root(func, access.base_var));
    }

    // Multiple unknown pointers — cannot prove disjoint.
    let unknown_count = roots.iter().filter(|r| **r == PointerRoot::Unknown).count();
    if unknown_count > 1 {
        return false;
    }
    if unknown_count == 1 && roots.len() > 1 {
        // e.g. unknown pointer + stack alloca — may alias.
        return false;
    }

    // Multiple allocas always allowed (distinct stack objects).
    true
}

/// Full memory safety gate for vectorization.
pub fn check_memory_dependence(
    func: &Function,
    loads: &[MemAccess],
    stores: &[MemAccess],
    vf: usize,
    arith_ops: &[(VarId, Operand, Operand)],
) -> bool {
    if vf < 2 {
        return true;
    }
    if !check_distinct_base_aliases(func, loads, stores) {
        return false;
    }
    check_same_base_dependences(loads, stores, vf, arith_ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(base: VarId, scale: i64, off: i64, dest: VarId) -> MemAccess {
        MemAccess {
            base_var: base,
            index_pattern: IndexPattern { scale, offset: off },
            is_load: true,
            data: Operand::Var(dest),
            dest: Some(dest),
        }
    }

    fn store(base: VarId, scale: i64, off: i64, src: VarId) -> MemAccess {
        MemAccess {
            base_var: base,
            index_pattern: IndexPattern { scale, offset: off },
            is_load: false,
            data: Operand::Var(src),
            dest: None,
        }
    }

    fn empty_func() -> Function {
        Function {
            name: "f".to_string(),
            return_type: model::Type::Int,
            params: vec![],
            entry_block: ir::BlockId(0),
            blocks: vec![],
            var_types: std::collections::HashMap::new(),
            attributes: vec![],
            is_static: false,
        }
    }

    #[test]
    fn simd_span_unit_stride() {
        let s = IndexPattern::direct().simd_span(4).unwrap();
        assert_eq!(s.min, 0);
        assert_eq!(s.max, 3);
    }

    #[test]
    fn rejects_stride_two_simd() {
        let pat = IndexPattern { scale: 2, offset: 0 };
        assert!(pat.simd_span(4).is_none());
    }

    #[test]
    fn copy_pattern_same_span_ok() {
        let func = empty_func();
        let l = load(VarId(1), 1, 0, VarId(2));
        let s = store(VarId(1), 1, 0, VarId(2));
        assert!(check_memory_dependence(&func, &[l], &[s], 4, &[]));
    }

    #[test]
    fn transform_same_span_ok() {
        let func = empty_func();
        let l = load(VarId(1), 1, 0, VarId(2));
        let s = store(VarId(1), 1, 0, VarId(3));
        let arith = [(VarId(3), Operand::Var(VarId(2)), Operand::Constant(1))];
        assert!(check_memory_dependence(&func, &[l], &[s], 4, &arith));
    }

    #[test]
    fn overlapping_offsets_rejected() {
        let func = empty_func();
        let l = load(VarId(1), 1, 0, VarId(2));
        let s = store(VarId(1), 1, 1, VarId(3));
        assert!(!check_memory_dependence(&func, &[l], &[s], 4, &[]));
    }

    #[test]
    fn shift_pattern_rejected() {
        let func = empty_func();
        let l = load(VarId(1), 1, 0, VarId(2));
        let s = store(VarId(1), 1, 1, VarId(3));
        assert!(!check_memory_dependence(&func, &[l], &[s], 4, &[]));
    }

    #[test]
    fn duplicate_store_slot_rejected() {
        let func = empty_func();
        let s1 = store(VarId(1), 1, 0, VarId(4));
        let s2 = store(VarId(1), 1, 0, VarId(5));
        assert!(!check_memory_dependence(&func, &[], &[s1, s2], 4, &[]));
    }

    #[test]
    fn different_alloca_bases_ok() {
        let mut func = empty_func();
        func.blocks.push(ir::BasicBlock {
            id: ir::BlockId(0),
            instructions: vec![
                Instruction::Alloca {
                    dest: VarId(10),
                    r#type: model::Type::Int,
                },
                Instruction::Alloca {
                    dest: VarId(11),
                    r#type: model::Type::Int,
                },
            ],
            terminator: ir::Terminator::Ret(None),
            is_label_target: false,
        });
        let l = load(VarId(10), 1, 0, VarId(2));
        let s = store(VarId(11), 1, 0, VarId(2));
        assert!(check_memory_dependence(&func, &[l], &[s], 4, &[]));
    }
}
