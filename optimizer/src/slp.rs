// Basic-block SLP: vectorize unrolled `dst[k] = src[k]` for k = 0..VF-1.

use ir::{BlockId, Function, Instruction, Operand, SimdOp, VarId};
use model::Type;
use std::collections::HashMap;

#[derive(Clone)]
struct GepInfo {
    base: VarId,
    index: i64,
    elem_type: Type,
}

/// Replace a fully-unrolled scalar copy cluster with one SIMD load + store.
pub fn slp_vectorize_function(func: &mut Function, vf: usize) {
    if vf < 2 {
        return;
    }
    let ids: Vec<BlockId> = func.blocks.iter().map(|b| b.id).collect();
    for id in ids {
        try_slp_copy_cluster(func, id, vf);
    }
}

fn try_slp_copy_cluster(func: &mut Function, block_id: BlockId, vf: usize) {
    let Some(block) = func.blocks.iter().find(|b| b.id == block_id) else {
        return;
    };
    if matches!(block.terminator, ir::Terminator::CondBr { .. }) {
        return;
    }

    let insts = &block.instructions;
    let mut gep_info: HashMap<VarId, GepInfo> = HashMap::new();
    let mut loads: HashMap<(VarId, i64), VarId> = HashMap::new(); // (base, idx) -> load dest
    let mut stores: Vec<(VarId, i64, VarId, Type, VarId)> = Vec::new(); // dst_base, idx, src, ty, gep

    for inst in insts {
        match inst {
            Instruction::GetElementPtr { dest, base, index, element_type } => {
                if let (Operand::Var(b), Operand::Constant(idx)) = (base, index) {
                    gep_info.insert(
                        *dest,
                        GepInfo {
                            base: *b,
                            index: *idx,
                            elem_type: element_type.clone(),
                        },
                    );
                }
            }
            Instruction::Load { dest, addr, value_type, .. } => {
                if let Operand::Var(gep) = addr {
                    if let Some(gi) = gep_info.get(gep) {
                        loads.insert((gi.base, gi.index), *dest);
                    }
                }
                let _ = value_type;
            }
            Instruction::Store { addr, src, value_type, .. } => {
                if let (Operand::Var(gep), Operand::Var(src_v)) = (addr, src) {
                    if let Some(gi) = gep_info.get(gep) {
                        stores.push((gi.base, gi.index, *src_v, value_type.clone(), *gep));
                    }
                }
            }
            _ => {}
        }
    }

    let Some(src_base) = loads
        .keys()
        .map(|(b, _)| *b)
        .find(|base| (0..vf as i64).all(|i| loads.contains_key(&(*base, i))))
    else {
        return;
    };

    let mut src_vals = Vec::with_capacity(vf);
    for i in 0..vf as i64 {
        let Some(v) = loads.get(&(src_base, i)) else {
            return;
        };
        src_vals.push(*v);
    }

    let Some(dst_candidate) = stores.iter().find(|(_, idx, src, _, _)| {
        *idx == 0 && src_vals.first() == Some(src)
    }) else {
        return;
    };
    let dst_base = dst_candidate.0;

    for i in 0..vf as i64 {
        let want_src = loads.get(&(src_base, i)).copied().unwrap();
        if !stores
            .iter()
            .any(|(b, idx, s, _, _)| *b == dst_base && *idx == i && *s == want_src)
        {
            return;
        }
    }

    let elem_type = gep_info
        .values()
        .find(|g| g.base == src_base && g.index == 0)
        .map(|g| g.elem_type.clone())
        .unwrap_or(Type::Int);

    let remove_geps: std::collections::HashSet<VarId> = gep_info
        .iter()
        .filter(|(_, g)| {
            (g.base == src_base || g.base == dst_base) && (0..vf as i64).contains(&g.index)
        })
        .map(|(d, _)| *d)
        .collect();
    let remove_loads: std::collections::HashSet<VarId> = src_vals.iter().copied().collect();

    let max_var = find_max_var_id(func);
    let gep_src = VarId(max_var + 1);
    let vec_load = VarId(max_var + 2);
    let gep_dst = VarId(max_var + 3);

    let new_head = vec![
        Instruction::GetElementPtr {
            dest: gep_src,
            base: Operand::Var(src_base),
            index: Operand::Constant(0),
            element_type: elem_type.clone(),
        },
        Instruction::Simd {
            op: SimdOp::Load,
            dest: Some(vec_load),
            operands: vec![Operand::Var(gep_src)],
            elem_type: elem_type.clone(),
            width: vf,
        },
        Instruction::GetElementPtr {
            dest: gep_dst,
            base: Operand::Var(dst_base),
            index: Operand::Constant(0),
            element_type: elem_type.clone(),
        },
        Instruction::Simd {
            op: SimdOp::Store,
            dest: None,
            operands: vec![Operand::Var(gep_dst), Operand::Var(vec_load)],
            elem_type,
            width: vf,
        },
    ];

    let Some(block) = func.blocks.iter_mut().find(|b| b.id == block_id) else {
        return;
    };
    block.instructions.retain(|inst| match inst {
        Instruction::GetElementPtr { dest, .. } => !remove_geps.contains(dest),
        Instruction::Load { dest, .. } => !remove_loads.contains(dest),
        Instruction::Store { addr, .. } => {
            if let Operand::Var(g) = addr {
                !remove_geps.contains(g)
            } else {
                true
            }
        }
        _ => true,
    });
    let mut combined = new_head;
    combined.append(&mut block.instructions);
    block.instructions = combined;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{BasicBlock, BlockId, Terminator};

    #[test]
    fn slp_replaces_four_scalar_copies() {
        let mut func = Function {
            name: "f".to_string(),
            return_type: Type::Int,
            params: vec![],
            entry_block: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(1),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Constant(0),
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
                        base: Operand::Var(VarId(10)),
                        index: Operand::Constant(1),
                        element_type: Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(4),
                        addr: Operand::Var(VarId(3)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Constant(2),
                        element_type: Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(6),
                        addr: Operand::Var(VarId(5)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(7),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Constant(3),
                        element_type: Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(8),
                        addr: Operand::Var(VarId(7)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(11),
                        base: Operand::Var(VarId(20)),
                        index: Operand::Constant(0),
                        element_type: Type::Int,
                    },
                    Instruction::Store {
                        addr: Operand::Var(VarId(11)),
                        src: Operand::Var(VarId(2)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(12),
                        base: Operand::Var(VarId(20)),
                        index: Operand::Constant(1),
                        element_type: Type::Int,
                    },
                    Instruction::Store {
                        addr: Operand::Var(VarId(12)),
                        src: Operand::Var(VarId(4)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(13),
                        base: Operand::Var(VarId(20)),
                        index: Operand::Constant(2),
                        element_type: Type::Int,
                    },
                    Instruction::Store {
                        addr: Operand::Var(VarId(13)),
                        src: Operand::Var(VarId(6)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                    Instruction::GetElementPtr {
                        dest: VarId(14),
                        base: Operand::Var(VarId(20)),
                        index: Operand::Constant(3),
                        element_type: Type::Int,
                    },
                    Instruction::Store {
                        addr: Operand::Var(VarId(14)),
                        src: Operand::Var(VarId(8)),
                        value_type: Type::Int,
                        volatile: false,
                    },
                ],
                terminator: Terminator::Ret(Some(Operand::Constant(0))),
                is_label_target: false,
            }],
            var_types: std::collections::HashMap::new(),
            attributes: vec![],
            is_static: false,
        };

        slp_vectorize_function(&mut func, 4);
        let block = &func.blocks[0];
        assert!(block.instructions.iter().any(|i| matches!(
            i,
            Instruction::Simd { op: SimdOp::Load, width: 4, .. }
        )));
        assert!(block.instructions.iter().any(|i| matches!(
            i,
            Instruction::Simd { op: SimdOp::Store, width: 4, .. }
        )));
    }
}

fn find_max_var_id(func: &Function) -> usize {
    let mut max = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Some(d) = inst.dest() {
                max = max.max(d.0);
            }
        }
    }
    max
}
