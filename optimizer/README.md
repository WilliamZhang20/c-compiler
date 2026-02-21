# Optimizer

The **Optimizer** crate runs a fixed sequence of transformation passes over the SSA-form IR to improve the efficiency of generated code without changing observable behavior. The single public entry point is `optimize(program: IRProgram) -> IRProgram`, which processes each function through the pipeline below.

## Pipeline Order

1. **mem2reg** (from the `ir` crate) — promotes stack allocations to SSA registers
2. Algebraic simplification
3. Strength reduction
4. Copy propagation
5. Common subexpression elimination
6. Constant folding + dead code elimination (integrated fixpoint)
7. Phi removal (from the `ir` crate) — lowers phi nodes into copies
8. CFG simplification

## Source Files

### `lib.rs`
Module root and pipeline driver. Declares submodules and invokes each pass in the order listed above on every function in the program.

### `algebraic.rs`
**Algebraic identity simplification.** Scans all `Binary` instructions and replaces them with `Copy` when a mathematical identity applies. Covers a wide range of patterns: `x * 0 → 0`, `x * 1 → x`, `x + 0 → x`, `x - x → 0`, `x & 0 → 0`, `x | 0 → x`, `x ^ x → 0`, `x << 0 → x`, `x == x → 1`, `x != x → 0`, and many more. Each arithmetic operator has a dedicated simplifier.

### `strength.rs`
**Strength reduction.** Replaces expensive integer operations whose right (or left, for commutative ops) operand is a power of two with cheaper equivalents:

- `x * 2^k` → `x << k`
- `x / 2^k` → `x >> k`
- `x % 2^k` → `x & (2^k - 1)`

Uses helpers from `utils.rs` for power-of-two detection and log₂ computation.

### `propagation.rs`
**Copy propagation.** Collects all `Copy` instructions into a map, transitively resolves chains (`x = y`, `y = z` → `x = z`) with cycle detection, then rewrites all operand references across instructions and terminators — including `FloatBinary` and `FloatUnary` variants. Dead copies whose destinations are no longer used are removed.

### `cse.rs`
**Common subexpression elimination.** Within each basic block, hashes `Binary` instructions by a canonical `(op, left, right)` key (with operand reordering for commutative ops) and replaces duplicates with a `Copy` of the first result. Operand replacement within `FloatBinary` and `FloatUnary` instructions is also handled. The expression map is reset at block boundaries to avoid invalid cross-block reuse.

### `folding.rs`
**Constant folding and propagation.** Runs a fixpoint loop (up to 10 iterations) interleaved with DCE. Maintains a per-block constant map; when both operands of a `Binary` resolve to known constants the result is evaluated at compile time. `Copy` of a constant propagates the value. Conditional branches (`CondBr`) with constant conditions are folded into unconditional branches. Covers all integer binary and unary operators (except `Assign` and logical short-circuit ops).

### `dce.rs`
**Dead code elimination.** Computes the set of used `VarId`s across all instructions and terminators, then removes pure instructions (`Binary`, `Unary`, `Copy`, `Cast`, `Load`, `GEP`, `Phi`) whose destination is not in the used set. Side-effecting instructions (`Call`, `Store`, `InlineAsm`, `Alloca`, variadic ops) are always retained.

### `cfg_simplify.rs`
**Control-flow graph simplification.** Iterates two sub-passes in a fixpoint loop:

- **Block merging** — when block A's only successor is block B and B's only predecessor is A (and B is not a goto target or phi-bearing), A absorbs B's instructions and terminator.
- **Empty block removal** — blocks with no instructions and an unconditional branch are bypassed: all incoming edges are redirected to the final target (with transitive closure and cycle detection).

Merged blocks are tombstoned (`Unreachable`) rather than removed to preserve `BlockId` indexing. Phi operands and terminators referencing removed blocks are updated.

### `utils.rs`
Shared helpers: `is_power_of_two(n)` and `log2(n)`, both marked `#[inline]`.
