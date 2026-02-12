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

use ir::IRProgram;
use algebraic::algebraic_simplification;
use strength::strength_reduce_function;
use propagation::copy_propagation;
use cse::common_subexpression_elimination;
use folding::optimize_function;

/// Main optimization entry point
///
/// Runs a series of optimization passes on each function in the program:
/// 1. Mem2reg - promote memory allocations to SSA registers
/// 2. Algebraic simplification - apply mathematical identities
/// 3. Strength reduction - replace expensive ops with cheaper ones
/// 4. Copy propagation - forward copy values
/// 5. Load forwarding - eliminate redundant memory loads
/// 6. Common subexpression elimination - remove redundant calculations
/// 7. Constant folding - evaluate constant expressions at compile time
/// 8. Dead code elimination - remove unused computations (integrated in folding)
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
        common_subexpression_elimination(func);
        optimize_function(func); // Includes constant folding and DCE
        ir::remove_phis(func);
    }
    program
}
