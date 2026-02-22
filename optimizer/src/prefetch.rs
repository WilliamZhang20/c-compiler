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

/// Minimum trip count to insert prefetches (not worth it for small loops)
const MIN_TRIP_COUNT: usize = 64;

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
                Instruction::Simd { dest: Some(d), .. } => {
                    max = max.max(d.0);
                }
                Instruction::InlineAsm { outputs, .. } => {
                    for o in outputs {
                        max = max.max(o.0);
                    }
                }
                _ => {}
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
}
