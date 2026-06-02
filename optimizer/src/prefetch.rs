// Software Prefetch Insertion
//
// Inserts prefetcht0 hints for array accesses inside loops to pre-load data
// into L1 cache before it's needed. This hides memory latency for sequential
// array access patterns.
//
// Strategy: For each Load from an IV-indexed GEP in a loop, insert a prefetch
// for the data PREFETCH_DISTANCE iterations ahead: prefetcht0 [base + (iv + dist) * elem_size]
//
// This only applies when:
// - The loop has a known induction variable
// - The load accesses memory through a GEP indexed by the IV
// - The trip count is large enough to benefit from prefetching (>= 64)

use ir::{Function, Instruction, Operand, VarId, BlockId};
use model::BinaryOp;
use crate::loop_analysis::{self, NaturalLoop};

/// Prefetch distance in elements (how far ahead to prefetch).
/// Tuned for L1 cache latency (~4 cycles on modern x86).
/// A distance of 16 elements × 4 bytes = 64 bytes = 1 cache line ahead.
const PREFETCH_DISTANCE: i64 = 16;

/// Minimum trip count to insert prefetches (not worth it for small loops).
/// Hardware prefetchers handle sequential access well for in-cache data,
/// so only insert software prefetches for very large iterations where the
/// working set likely exceeds L2 cache significantly.
const MIN_TRIP_COUNT: usize = 100_000;

/// Run software prefetch insertion on all loops in a function
pub fn insert_prefetches(func: &mut Function) {
    let loops = loop_analysis::find_loops(func);
    for lp in &loops {
        insert_prefetch_for_loop(func, lp);
    }
}

/// Information about a memory access through an IV-indexed GEP
struct IvMemAccess {
    /// The block where the load occurs
    block_id: BlockId,
    /// Index of the load instruction in the block
    _load_idx: usize,
    /// The GEP's base variable
    gep_base: Operand,
    /// Element type of the GEP
    gep_elem_type: model::Type,
    /// The induction variable used as the GEP index
    iv_var: VarId,
}

/// Insert prefetches for a single loop
fn insert_prefetch_for_loop(func: &mut Function, lp: &NaturalLoop) {
    let iv = match &lp.induction_var {
        Some(iv) => iv.clone(),
        None => return,
    };

    let trip_count = match lp.trip_count {
        Some(tc) => tc,
        None => return,
    };

    // Only prefetch for loops with enough iterations
    if trip_count < MIN_TRIP_COUNT {
        return;
    }

    // Find all loads from IV-indexed GEPs
    let accesses = find_iv_mem_accesses(func, lp, iv.var);
    if accesses.is_empty() {
        return;
    }

    // Find max var ID for creating new temporaries
    let mut next_var = find_max_var_id(func) + 1;

    // For each access, insert a prefetch instruction
    for access in &accesses {
        // Create: prefetch_iv = iv + PREFETCH_DISTANCE
        let prefetch_iv = VarId(next_var);
        next_var += 1;

        // Create: prefetch_gep = GEP(base, prefetch_iv)
        let prefetch_gep = VarId(next_var);
        next_var += 1;

        let new_insts = vec![
            // prefetch_iv = iv + PREFETCH_DISTANCE
            Instruction::Binary {
                dest: prefetch_iv,
                op: BinaryOp::Add,
                left: Operand::Var(access.iv_var),
                right: Operand::Constant(PREFETCH_DISTANCE),
            },
            // prefetch_gep = &base[prefetch_iv]
            Instruction::GetElementPtr {
                dest: prefetch_gep,
                base: access.gep_base.clone(),
                index: Operand::Var(prefetch_iv),
                element_type: access.gep_elem_type.clone(),
            },
            // prefetcht0 [prefetch_gep]
            Instruction::InlineAsm {
                template: "prefetcht0 [%0]".to_string(),
                outputs: vec![],
                inputs: vec![Operand::Var(prefetch_gep)],
                output_constraints: vec![],
                input_constraints: vec!["r".to_string()],
                clobbers: vec![],
                is_volatile: true,
            },
        ];

        // Insert the prefetch instructions at the beginning of the block
        // (before the load, but after any Phi nodes)
        if let Some(block) = func.blocks.iter_mut().find(|b| b.id == access.block_id) {
            // Find insertion point: after all Phi nodes
            let insert_pos = block.instructions.iter()
                .position(|inst| !matches!(inst, Instruction::Phi { .. }))
                .unwrap_or(0);

            for (i, inst) in new_insts.into_iter().enumerate() {
                block.instructions.insert(insert_pos + i, inst);
            }
        }
    }
}

/// Find all loads in the loop body that access memory through IV-indexed GEPs
fn find_iv_mem_accesses(
    func: &Function,
    lp: &NaturalLoop,
    iv_var: VarId,
) -> Vec<IvMemAccess> {
    let mut accesses = Vec::new();

    // First, collect all GEP instructions in the loop body that use the IV
    let mut iv_geps: std::collections::HashMap<VarId, (Operand, model::Type)> = std::collections::HashMap::new();

    for block in &func.blocks {
        if !lp.body.contains(&block.id) {
            continue;
        }
        for inst in &block.instructions {
            if let Instruction::GetElementPtr { dest, base, index, element_type } = inst {
                if matches!(index, Operand::Var(v) if *v == iv_var) {
                    iv_geps.insert(*dest, (base.clone(), element_type.clone()));
                }
            }
        }
    }

    // Now find loads from those GEPs
    for block in &func.blocks {
        if !lp.body.contains(&block.id) {
            continue;
        }
        for (idx, inst) in block.instructions.iter().enumerate() {
            if let Instruction::Load { addr, .. } = inst {
                if let Operand::Var(addr_var) = addr {
                    if let Some((base, elem_type)) = iv_geps.get(addr_var) {
                        accesses.push(IvMemAccess {
                            block_id: block.id,
                            _load_idx: idx,
                            gep_base: base.clone(),
                            gep_elem_type: elem_type.clone(),
                            iv_var,
                        });
                    }
                }
            }
        }
    }

    accesses
}

fn find_max_var_id(func: &Function) -> usize {
    let mut max = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            for d in inst.dests() {
                max = max.max(d.0);
            }
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_to_ir(src: &str) -> ir::IRProgram {
        let tokens = lexer::lex(src).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        lowerer.lower_program(&ast).unwrap()
    }

    #[test]
    fn test_prefetch_large_array_loop() {
        let src = r#"
            int main() {
                int arr[1000];
                int sum = 0;
                int i;
                for (i = 0; i < 1000; i = i + 1) {
                    arr[i] = i;
                }
                for (i = 0; i < 1000; i = i + 1) {
                    sum = sum + arr[i];
                }
                return sum % 256;
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            insert_prefetches(func);
        }
        // Should not crash; prefetches should be inserted for arr[i] accesses
    }

    #[test]
    fn test_no_prefetch_small_loop() {
        let src = r#"
            int main() {
                int arr[10];
                int i;
                for (i = 0; i < 10; i = i + 1) {
                    arr[i] = i;
                }
                return arr[5];
            }
        "#;
        let mut prog = compile_to_ir(src);
        for func in &mut prog.functions {
            ir::mem2reg(func);
            insert_prefetches(func);
        }
        // Should not insert prefetches for small loops
    }

    // ─── find_max_var_id ────────────────────────────────────────

    fn make_func(blocks: Vec<ir::BasicBlock>) -> Function {
        use std::collections::HashMap;
        Function {
            name: "test".to_string(),
            return_type: model::Type::Int,
            params: vec![],
            entry_block: BlockId(0),
            blocks,
            var_types: HashMap::new(),
            attributes: vec![],
            is_static: false,
        }
    }

    fn ret_block(id: usize) -> ir::BasicBlock {
        ir::BasicBlock {
            id: BlockId(id),
            instructions: vec![],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }
    }

    // ─── find_max_var_id ────────────────────────────────────────

    #[test]
    fn test_find_max_var_id_basic() {
        let func = make_func(vec![ir::BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Binary {
                    dest: VarId(3),
                    op: model::BinaryOp::Add,
                    left: Operand::Constant(1),
                    right: Operand::Constant(2),
                },
                Instruction::Copy { dest: VarId(7), src: Operand::Constant(0) },
                Instruction::Load {
                    dest: VarId(2),
                    addr: Operand::Var(VarId(1)),
                    value_type: model::Type::Int,
                    volatile: false,
                },
            ],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 7);
    }

    #[test]
    fn test_find_max_var_id_with_gep_phi_cast_alloca() {
        let func = make_func(vec![ir::BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Alloca {
                    dest: VarId(1),
                    r#type: model::Type::Int,
                },
                Instruction::GetElementPtr {
                    dest: VarId(5),
                    base: Operand::Var(VarId(1)),
                    index: Operand::Constant(0),
                    element_type: model::Type::Int,
                },
                Instruction::Phi {
                    dest: VarId(8),
                    preds: vec![(BlockId(0), VarId(0))],
                },
                Instruction::Cast {
                    dest: VarId(3),
                    src: Operand::Var(VarId(1)),
                    r#type: model::Type::Long,
                },
                Instruction::Unary {
                    dest: VarId(10),
                    op: model::UnaryOp::Minus,
                    src: Operand::Constant(5),
                },
            ],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 10);
    }

    #[test]
    fn test_find_max_var_id_with_calls() {
        let func = make_func(vec![ir::BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Call {
                    dest: Some(VarId(15)),
                    name: "foo".to_string(),
                    args: vec![],
                },
                Instruction::InlineAsm {
                    template: "nop".to_string(),
                    outputs: vec![VarId(20)],
                    inputs: vec![],
                    output_constraints: vec!["=r".to_string()],
                    input_constraints: vec![],
                    clobbers: vec![],
                    is_volatile: false,
                },
            ],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 20);
    }

    #[test]
    fn test_find_max_var_id_with_simd() {
        let func = make_func(vec![ir::BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Simd {
                    op: ir::SimdOp::Load,
                    dest: Some(VarId(25)),
                    operands: vec![Operand::Var(VarId(1))],
                    elem_type: model::Type::Int,
                    width: 8,
                },
            ],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        assert_eq!(find_max_var_id(&func), 25);
    }

    #[test]
    fn test_find_max_var_id_with_store() {
        let func = make_func(vec![ir::BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Store {
                    addr: Operand::Var(VarId(1)),
                    src: Operand::Constant(42),
                    value_type: model::Type::Int,
                    volatile: false,
                },
            ],
            terminator: ir::Terminator::Ret(Some(Operand::Constant(0))),
            is_label_target: false,
        }]);
        // Store has no dest → max stays 0
        assert_eq!(find_max_var_id(&func), 0);
    }

    // ─── find_iv_mem_accesses ───────────────────────────────────

    #[test]
    fn test_find_iv_mem_accesses_basic() {
        use crate::loop_analysis::NaturalLoop;

        let func = make_func(vec![
            ir::BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
            ir::BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(0)), // IV
                        element_type: model::Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(6),
                        addr: Operand::Var(VarId(5)),
                        value_type: model::Type::Int,
                        volatile: false,
                    },
                ],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let lp = NaturalLoop {
            header: BlockId(1),
            latch: BlockId(1),
            body: vec![BlockId(1)].into_iter().collect(),
            exit: None,
            preheader: Some(BlockId(0)),
            induction_var: None,
            trip_count: None,
        };

        let result = find_iv_mem_accesses(&func, &lp, VarId(0));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].iv_var, VarId(0));
        assert_eq!(result[0].block_id, BlockId(1));
    }

    #[test]
    fn test_find_iv_mem_accesses_no_match() {
        use crate::loop_analysis::NaturalLoop;

        let func = make_func(vec![
            ir::BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(99)), // NOT the IV
                        element_type: model::Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(6),
                        addr: Operand::Var(VarId(5)),
                        value_type: model::Type::Int,
                        volatile: false,
                    },
                ],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let lp = NaturalLoop {
            header: BlockId(1),
            latch: BlockId(1),
            body: vec![BlockId(1)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: None,
            trip_count: None,
        };

        let result = find_iv_mem_accesses(&func, &lp, VarId(0));
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_iv_mem_accesses_load_not_from_gep() {
        use crate::loop_analysis::NaturalLoop;

        let func = make_func(vec![
            ir::BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(5),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(0)),
                        element_type: model::Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(6),
                        addr: Operand::Var(VarId(99)), // not from VarId(5)
                        value_type: model::Type::Int,
                        volatile: false,
                    },
                ],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
        ]);

        let lp = NaturalLoop {
            header: BlockId(1),
            latch: BlockId(1),
            body: vec![BlockId(1)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: None,
            trip_count: None,
        };

        let result = find_iv_mem_accesses(&func, &lp, VarId(0));
        assert!(result.is_empty());
    }

    // ─── insert_prefetch_for_loop: full path ────────────────────

    #[test]
    fn test_insert_prefetch_for_loop_manual_ir() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let mut func = make_func(vec![
            ir::BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
            ir::BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    Instruction::Phi {
                        dest: VarId(0),
                        preds: vec![
                            (BlockId(0), VarId(100)),
                            (BlockId(2), VarId(3)),
                        ],
                    },
                ],
                terminator: ir::Terminator::cond_br(
                    Operand::Var(VarId(0)),
                    BlockId(2),
                    BlockId(3),
                ),
                is_label_target: false,
            },
            ir::BasicBlock {
                id: BlockId(2),
                instructions: vec![
                    Instruction::GetElementPtr {
                        dest: VarId(1),
                        base: Operand::Var(VarId(10)),
                        index: Operand::Var(VarId(0)),
                        element_type: model::Type::Int,
                    },
                    Instruction::Load {
                        dest: VarId(2),
                        addr: Operand::Var(VarId(1)),
                        value_type: model::Type::Int,
                        volatile: false,
                    },
                    Instruction::Binary {
                        dest: VarId(3),
                        op: model::BinaryOp::Add,
                        left: Operand::Var(VarId(0)),
                        right: Operand::Constant(1),
                    },
                ],
                terminator: ir::Terminator::Br(BlockId(1)),
                is_label_target: false,
            },
            ret_block(3),
        ]);

        let lp = NaturalLoop {
            header: BlockId(1),
            latch: BlockId(2),
            body: vec![BlockId(1), BlockId(2)].into_iter().collect(),
            exit: Some(BlockId(3)),
            preheader: Some(BlockId(0)),
            induction_var: Some(InductionVar {
                var: VarId(0),
                init: 0,
                step: 1,
                bound: 100,
                bound_operand: Operand::Constant(100),
                cmp_op: model::BinaryOp::Less,
            }),
            trip_count: Some(200_000),
        };

        let original_inst_count = func.blocks[2].instructions.len();
        insert_prefetch_for_loop(&mut func, &lp);

        // Should have inserted 3 new instructions (Binary + GEP + InlineAsm) in body block
        let new_inst_count = func.blocks[2].instructions.len();
        assert_eq!(new_inst_count, original_inst_count + 3);

        // Check that an InlineAsm with prefetcht0 was inserted
        let has_prefetch = func.blocks[2].instructions.iter().any(|inst| {
            matches!(inst, Instruction::InlineAsm { template, .. } if template.contains("prefetcht0"))
        });
        assert!(has_prefetch, "Expected prefetcht0 instruction in body block");
    }

    #[test]
    fn test_insert_prefetch_no_iv() {
        use crate::loop_analysis::NaturalLoop;

        let mut func = make_func(vec![ret_block(0)]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(0),
            body: vec![BlockId(0)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: None,
            trip_count: Some(100),
        };

        insert_prefetch_for_loop(&mut func, &lp);
        assert_eq!(func.blocks[0].instructions.len(), 0);
    }

    #[test]
    fn test_insert_prefetch_no_trip_count() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let mut func = make_func(vec![ret_block(0)]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(0),
            body: vec![BlockId(0)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 100,
                bound_operand: Operand::Constant(100),
                cmp_op: model::BinaryOp::Less,
            }),
            trip_count: None,
        };

        insert_prefetch_for_loop(&mut func, &lp);
        assert_eq!(func.blocks[0].instructions.len(), 0);
    }

    #[test]
    fn test_insert_prefetch_small_trip_count() {
        use crate::loop_analysis::{NaturalLoop, InductionVar};

        let mut func = make_func(vec![ret_block(0)]);

        let lp = NaturalLoop {
            header: BlockId(0),
            latch: BlockId(0),
            body: vec![BlockId(0)].into_iter().collect(),
            exit: None,
            preheader: None,
            induction_var: Some(InductionVar {
                var: VarId(0), init: 0, step: 1, bound: 10,
                bound_operand: Operand::Constant(10),
                cmp_op: model::BinaryOp::Less,
            }),
            trip_count: Some(10),
        };

        insert_prefetch_for_loop(&mut func, &lp);
        assert_eq!(func.blocks[0].instructions.len(), 0);
    }
}
