// Optimizer module: IR optimization passes for improving code quality and performance
//
// Architecture: Each optimization is a `FunctionPass` registered with a
// `PassManager`.  The pipeline is built in `default_pipeline()`.  Adding a
// new pass only requires implementing the trait and appending one entry –
// no existing code needs editing.
//
// Module organization:
// - algebraic.rs: Algebraic simplification (x*0=0, x+0=x, etc.)
// - strength.rs: Strength reduction (multiply by power of 2 → shift)
// - propagation.rs: Copy propagation (replace uses with copy sources)
// - cse.rs: Common subexpression elimination
// - dce.rs: Dead code elimination (remove unused computations)
// - folding.rs: Constant folding and propagation
// - load_forwarding.rs: Eliminate redundant loads from same memory location
// - utils.rs: Utility functions (is_power_of_two, etc.)

mod algebraic;
mod strength;
mod propagation;
mod cse;
mod dce;
mod folding;
mod utils;
mod cfg_simplify;
mod load_forwarding;
mod licm;
mod prefetch;
mod block_layout;
mod loop_interchange;
pub mod loop_analysis;
pub mod vectorize;
mod mem_dependence;
mod polyhedral;
mod slp;
mod inline;
mod profile;
mod recurrence;
mod sroa;

use ir::IRProgram;
use recurrence::eliminate_linear_recurrences;
use sroa::scalar_replacement_of_aggregates;
use algebraic::algebraic_simplification;
use strength::strength_reduce_function;
use propagation::copy_propagation;
use cse::common_subexpression_elimination;
use folding::optimize_function;
use cfg_simplify::simplify_cfg;
use load_forwarding::load_forwarding;
use licm::loop_invariant_code_motion;
use prefetch::insert_prefetches;
use block_layout::optimize_block_layout;
use loop_interchange::try_loop_interchange;
use model::target::SimdLevel;

// ═══════════════════════════════════════════════════════════════════
//  Pass trait + PassManager
// ═══════════════════════════════════════════════════════════════════

/// A single optimization pass that operates on one IR function at a time.
///
/// Implement this trait to add a new optimization.  Then register it in
/// `default_pipeline()` via `PassManager::add_pass()`.
pub trait FunctionPass {
    /// Human-readable name for diagnostics / debugging.
    fn name(&self) -> &str;

    /// Apply the pass to a single function, mutating it in place.
    fn run(&self, func: &mut ir::Function);
}

/// Ordered collection of `FunctionPass` objects that runs each pass on every
/// function in the program.
pub struct PassManager {
    passes: Vec<Box<dyn FunctionPass>>,
}

impl PassManager {
    pub fn new() -> Self {
        PassManager { passes: Vec::new() }
    }

    /// Append a pass to the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn FunctionPass>) {
        self.passes.push(pass);
    }

    /// Run every registered pass, in order, on every function in the program.
    pub fn run(&self, program: &mut IRProgram) {
        for func in &mut program.functions {
            for pass in &self.passes {
                pass.run(func);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  Concrete pass wrappers
// ═══════════════════════════════════════════════════════════════════

struct RecurrenceElimination;
impl FunctionPass for RecurrenceElimination {
    fn name(&self) -> &str { "recurrence-elimination" }
    fn run(&self, func: &mut ir::Function) { eliminate_linear_recurrences(func); }
}

struct SROA;
impl FunctionPass for SROA {
    fn name(&self) -> &str { "sroa" }
    fn run(&self, func: &mut ir::Function) { scalar_replacement_of_aggregates(func); }
}

struct Mem2Reg;
impl FunctionPass for Mem2Reg {
    fn name(&self) -> &str { "mem2reg" }
    fn run(&self, func: &mut ir::Function) { ir::mem2reg(func); }
}

struct AlgebraicSimplification;
impl FunctionPass for AlgebraicSimplification {
    fn name(&self) -> &str { "algebraic-simplification" }
    fn run(&self, func: &mut ir::Function) { algebraic_simplification(func); }
}

struct StrengthReduction;
impl FunctionPass for StrengthReduction {
    fn name(&self) -> &str { "strength-reduction" }
    fn run(&self, func: &mut ir::Function) { strength_reduce_function(func); }
}

struct CopyPropagation;
impl FunctionPass for CopyPropagation {
    fn name(&self) -> &str { "copy-propagation" }
    fn run(&self, func: &mut ir::Function) { copy_propagation(func); }
}

struct LoadForwarding;
impl FunctionPass for LoadForwarding {
    fn name(&self) -> &str { "load-forwarding" }
    fn run(&self, func: &mut ir::Function) { load_forwarding(func); }
}

struct CommonSubexprElim;
impl FunctionPass for CommonSubexprElim {
    fn name(&self) -> &str { "cse" }
    fn run(&self, func: &mut ir::Function) { common_subexpression_elimination(func); }
}

struct FoldingAndDCE;
impl FunctionPass for FoldingAndDCE {
    fn name(&self) -> &str { "folding-dce" }
    fn run(&self, func: &mut ir::Function) { optimize_function(func); }
}

struct LoopInterchange;
impl FunctionPass for LoopInterchange {
    fn name(&self) -> &str { "loop-interchange" }
    fn run(&self, func: &mut ir::Function) { try_loop_interchange(func); }
}

struct LICM;
impl FunctionPass for LICM {
    fn name(&self) -> &str { "licm" }
    fn run(&self, func: &mut ir::Function) { loop_invariant_code_motion(func); }
}

struct Prefetch;
impl FunctionPass for Prefetch {
    fn name(&self) -> &str { "prefetch" }
    fn run(&self, func: &mut ir::Function) { insert_prefetches(func); }
}

struct Vectorize {
    level: vectorize::SimdLevel,
}
impl FunctionPass for Vectorize {
    fn name(&self) -> &str { "vectorize" }
    fn run(&self, func: &mut ir::Function) {
        vectorize::vectorize_function(func, self.level);
    }
}

struct SlpVectorize {
    vf: usize,
}
impl FunctionPass for SlpVectorize {
    fn name(&self) -> &str { "slp" }
    fn run(&self, func: &mut ir::Function) {
        slp::slp_vectorize_function(func, self.vf);
    }
}

struct RemovePhis;
impl FunctionPass for RemovePhis {
    fn name(&self) -> &str { "remove-phis" }
    fn run(&self, func: &mut ir::Function) { ir::remove_phis(func); }
}

struct CfgSimplify;
impl FunctionPass for CfgSimplify {
    fn name(&self) -> &str { "cfg-simplify" }
    fn run(&self, func: &mut ir::Function) { simplify_cfg(func); }
}

struct BlockLayout;
impl FunctionPass for BlockLayout {
    fn name(&self) -> &str { "block-layout" }
    fn run(&self, func: &mut ir::Function) { optimize_block_layout(func); }
}

// ═══════════════════════════════════════════════════════════════════
//  Pipeline construction
// ═══════════════════════════════════════════════════════════════════

/// Build the default optimization pipeline for the given SIMD capability.
pub fn default_pipeline(simd_level: SimdLevel) -> PassManager {
    let mut pm = PassManager::new();

    // ── Round 1: initial optimization ───────────────────────────
    pm.add_pass(Box::new(SROA));
    pm.add_pass(Box::new(Mem2Reg));
    pm.add_pass(Box::new(AlgebraicSimplification));
    pm.add_pass(Box::new(StrengthReduction));
    pm.add_pass(Box::new(CopyPropagation));
    pm.add_pass(Box::new(LoadForwarding));
    pm.add_pass(Box::new(CommonSubexprElim));
    pm.add_pass(Box::new(FoldingAndDCE));
    pm.add_pass(Box::new(LoopInterchange));
    pm.add_pass(Box::new(LICM));
    pm.add_pass(Box::new(Prefetch));
    if simd_level >= SimdLevel::SSE2 {
        let vec_level = match simd_level {
            SimdLevel::AVX2 | SimdLevel::AVX => vectorize::SimdLevel::AVX2,
            _ => vectorize::SimdLevel::SSE2,
        };
        pm.add_pass(Box::new(Vectorize { level: vec_level }));
        pm.add_pass(Box::new(SlpVectorize { vf: vec_level.vector_width() }));
    }

    // ── Round 2: clean up after LICM / vectorize / etc. ────────
    pm.add_pass(Box::new(AlgebraicSimplification));
    pm.add_pass(Box::new(StrengthReduction));
    pm.add_pass(Box::new(CopyPropagation));
    pm.add_pass(Box::new(LoadForwarding));
    pm.add_pass(Box::new(CommonSubexprElim));
    pm.add_pass(Box::new(FoldingAndDCE));

    // ── Finalize ────────────────────────────────────────────────
    // Transform linear sum recurrences after other opts; re-SSA before phi removal.
    pm.add_pass(Box::new(RecurrenceElimination));
    pm.add_pass(Box::new(RemovePhis));
    pm.add_pass(Box::new(CfgSimplify));
    pm.add_pass(Box::new(BlockLayout));
    pm
}

// ═══════════════════════════════════════════════════════════════════
//  Public entry points
// ═══════════════════════════════════════════════════════════════════

pub use profile::{load_profile, write_profile, apply_profile_layout, BlockProfile, profile_counter_name};

/// Main optimization entry point (auto-detects SIMD level).
pub fn optimize(program: IRProgram) -> IRProgram {
    optimize_with_options(program, SimdLevel::detect(), None)
}

/// Optimize with explicit SIMD level control.
pub fn optimize_with_simd(mut program: IRProgram, simd_level: SimdLevel) -> IRProgram {
    optimize_with_options(program, simd_level, None)
}

/// Optimize with optional PGO profile data for block layout.
pub fn optimize_with_options(
    mut program: IRProgram,
    simd_level: SimdLevel,
    profile: Option<BlockProfile>,
) -> IRProgram {
    inline::inline_functions(&mut program);

    let pipeline = default_pipeline(simd_level);
    pipeline.run(&mut program);

    if let Some(ref prof) = profile {
        apply_profile_layout(&mut program, prof);
    }
    program
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{Instruction, Operand, Terminator};

    /// Helper: compile source to optimized IR
    fn compile_to_ir(src: &str) -> IRProgram {
        let tokens = lexer::lex(src).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        let ir_prog = lowerer.lower_program(&ast).unwrap();
        optimize(ir_prog)
    }

    /// Helper: get the first (main) function's instructions flat
    fn all_instructions(prog: &IRProgram) -> Vec<&Instruction> {
        prog.functions[0].blocks.iter()
            .flat_map(|b| b.instructions.iter())
            .collect()
    }

    #[test]
    fn constant_folding_simple() {
        let ir = compile_to_ir("int main() { return 3 + 4; }");
        // After constant folding, the return should be Ret(Constant(7))
        let f = &ir.functions[0];
        let ret = &f.blocks.iter()
            .find(|b| matches!(b.terminator, Terminator::Ret(_)))
            .unwrap().terminator;
        if let Terminator::Ret(Some(op)) = ret {
            assert_eq!(*op, Operand::Constant(7), "3+4 should fold to 7");
        } else {
            panic!("Expected Ret with value");
        }
    }

    #[test]
    fn constant_folding_nested() {
        let ir = compile_to_ir("int main() { return (2 * 3) + (10 - 4); }");
        let f = &ir.functions[0];
        let ret = &f.blocks.iter()
            .find(|b| matches!(b.terminator, Terminator::Ret(_)))
            .unwrap().terminator;
        if let Terminator::Ret(Some(op)) = ret {
            assert_eq!(*op, Operand::Constant(12), "(2*3)+(10-4) = 12");
        } else {
            panic!("Expected Ret with value");
        }
    }

    #[test]
    fn strength_reduction_multiply_by_power_of_two() {
        let ir = compile_to_ir("int f(int x) { return x * 8; }");
        let instrs = all_instructions(&ir);
        // Should not have a multiply instruction, should have a shift left
        let has_mul = instrs.iter().any(|i| matches!(i,
            Instruction::Binary { op: model::BinaryOp::Mul, .. }
        ));
        let has_shift = instrs.iter().any(|i| matches!(i,
            Instruction::Binary { op: model::BinaryOp::ShiftLeft, .. }
        ));
        assert!(!has_mul, "x * 8 should be strength-reduced (no Mul)");
        assert!(has_shift, "x * 8 should become x << 3");
    }

    #[test]
    fn algebraic_add_zero() {
        let ir = compile_to_ir("int f(int x) { return x + 0; }");
        let instrs = all_instructions(&ir);
        // x + 0 should be simplified away (no Add instruction)
        let has_add = instrs.iter().any(|i| matches!(i,
            Instruction::Binary { op: model::BinaryOp::Add, .. }
        ));
        assert!(!has_add, "x + 0 should be simplified away");
    }

    #[test]
    fn algebraic_multiply_by_one() {
        let ir = compile_to_ir("int f(int x) { return x * 1; }");
        let instrs = all_instructions(&ir);
        let has_mul = instrs.iter().any(|i| matches!(i,
            Instruction::Binary { op: model::BinaryOp::Mul, .. }
        ));
        assert!(!has_mul, "x * 1 should be simplified away");
    }

    #[test]
    fn algebraic_subtract_self() {
        // When the same SSA variable is subtracted from itself, it should fold to 0.
        // Use a local variable (not a parameter) so both uses have the same VarId.
        let ir = compile_to_ir("int main() { int x = 5; return x - x; }");
        let f = &ir.functions[0];
        let ret = &f.blocks.iter()
            .find(|b| matches!(b.terminator, Terminator::Ret(_)))
            .unwrap().terminator;
        if let Terminator::Ret(Some(op)) = ret {
            // After constant folding: 5 - 5 = 0
            assert_eq!(*op, Operand::Constant(0), "x - x should be 0");
        } else {
            panic!("Expected Ret with value");
        }
    }

    #[test]
    fn dead_code_eliminated() {
        let ir = compile_to_ir("int main() { int x = 5; int y = 10; return x; }");
        let instrs = all_instructions(&ir);
        // The assignment to y (value 10) should be dead and eliminated.
        // After optimization, we shouldn't see Constant(10) used in any Copy.
        let has_y_copy = instrs.iter().any(|i| matches!(i,
            Instruction::Copy { src: Operand::Constant(10), .. }
        ));
        assert!(!has_y_copy, "Dead variable y=10 should be eliminated");
    }

    #[test]
    fn optimizer_does_not_crash_on_empty_function() {
        let ir = compile_to_ir("void f() { } int main() { return 0; }");
        assert!(!ir.functions.is_empty());
    }

    #[test]
    fn optimizer_handles_loops() {
        // Make sure optimizer doesn't break loop control flow
        let ir = compile_to_ir("int main() { int s = 0; int i = 0; while (i < 10) { s = s + i; i = i + 1; } return s; }");
        assert!(!ir.functions.is_empty());
        // Should have at least a header, body, and exit block
        assert!(ir.functions[0].blocks.len() >= 3);
    }

    #[test]
    fn optimizer_handles_if_else() {
        let ir = compile_to_ir("int f(int x) { if (x > 0) { return 1; } else { return 0; } }");
        assert!(!ir.functions.is_empty());
    }

    #[test]
    fn nested_struct_ssa_after_sroa_and_mem2reg() {
        let src = include_str!("../../testing/test_nested_struct.c");
        let tokens = lexer::lex(src).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        let mut ir_prog = lowerer.lower_program(&ast).unwrap();
        for func in &mut ir_prog.functions {
            if func.name != "main" {
                continue;
            }
            assert!(
                ir::verify_ssa(func).is_ok(),
                "before SROA: {}",
                ir::verify_ssa(func).unwrap_err()
            );
            scalar_replacement_of_aggregates(func);
            assert!(
                ir::verify_ssa(func).is_ok(),
                "after SROA: {}",
                ir::verify_ssa(func).unwrap_err()
            );
            ir::mem2reg(func);
            assert!(
                ir::verify_ssa(func).is_ok(),
                "after mem2reg: {}",
                ir::verify_ssa(func).unwrap_err()
            );
        }
    }
}
