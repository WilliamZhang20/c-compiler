// Optimizer module: IR optimization passes for improving code quality and performance
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

use ir::IRProgram;
use algebraic::algebraic_simplification;
use strength::strength_reduce_function;
use propagation::copy_propagation;
use cse::common_subexpression_elimination;
use folding::optimize_function;
use cfg_simplify::simplify_cfg;
use load_forwarding::load_forwarding;

/// Main optimization entry point
///
/// Runs a series of optimization passes on each function in the program:
/// 1. Mem2reg - promote memory allocations to SSA registers
/// 2. CFG simplification - merge blocks and eliminate empty jumps
/// 3. Algebraic simplification - apply mathematical identities
/// 4. Strength reduction - replace expensive ops with cheaper ones
/// 5. Copy propagation - forward copy values
/// 6. Load forwarding - eliminate redundant memory loads
/// 7. Common subexpression elimination - remove redundant calculations
/// 8. Constant folding - evaluate constant expressions at compile time
/// 9. Dead code elimination - remove unused computations (integrated in folding)
/// 10. CFG simplification (again) - clean up after optimizations
///
/// # Arguments
/// * `program` - The IR program to optimize
///
/// # Returns
/// * Optimized IR program with improved code quality and performance
pub fn optimize(mut program: IRProgram) -> IRProgram {
    for func in &mut program.functions {
        ir::mem2reg(func);
        algebraic_simplification(func);
        strength_reduce_function(func);
        copy_propagation(func);
        load_forwarding(func);
        common_subexpression_elimination(func);
        optimize_function(func); // Includes constant folding and DCE
        ir::remove_phis(func);
        simplify_cfg(func);  // Run AFTER phi removal (only uses remove_empty_blocks now)
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
}
