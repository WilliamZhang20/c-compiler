# Optimizer

The **Optimizer** runs a fixed sequence of transformation passes over the SSA-form IR, improving code efficiency without changing observable behavior.

**Public API**: `optimizer::optimize(program: IRProgram) -> IRProgram`

Each function is processed independently through the full pipeline.

## Pipeline

The passes execute in this order for each function:

| # | Pass | File | What it does |
|---|---|---|---|
| 1 | mem2reg | `ir` crate | Promotes `alloca`/`load`/`store` of scalar locals to SSA registers via phi-node insertion |
| 2 | Algebraic simplification | `algebraic.rs` | Replaces identity operations with copies (see below) |
| 3 | Strength reduction | `strength.rs` | Replaces expensive ops with cheaper equivalents |
| 4 | Copy propagation | `propagation.rs` | Resolves copy chains; removes dead copies |
| 5 | Load forwarding | `load_forwarding.rs` | Replaces loads with previously stored values |
| 6 | Common subexpression elimination | `cse.rs` | Deduplicates identical computations within basic blocks |
| 7 | Constant folding + DCE | `folding.rs` + `dce.rs` | Evaluates compile-time constants; removes dead code |
| 8 | Loop interchange | `loop_interchange.rs` | Swaps nested loop order for sequential memory access |
| 9 | LICM | `licm.rs` | Hoists loop-invariant computations to preheader |
| 10 | Prefetch insertion | `prefetch.rs` | Inserts software prefetch hints for array loops |
| 11 | Auto-vectorization | `vectorize.rs` | Converts scalar loops to SIMD (SSE2/AVX2) |
| 12 | Phi removal | `ir` crate | Lowers phi nodes into copies at predecessor block ends |
| 13 | CFG simplification | `cfg_simplify.rs` | Merges blocks; removes dead blocks; bypasses empty blocks |
| 14 | Block layout | `block_layout.rs` | Reorders blocks for instruction cache locality |

The pipeline runs a single pass (multi-pass iteration was found to cause codegen issues with float function pointers).

## Source files

### `loop_analysis.rs` — Loop detection and analysis
Provides the loop analysis infrastructure used by LICM, vectorization, prefetching, and loop interchange. Computes dominators via iterative dataflow, identifies natural loops from back edges, derives loop bodies/exits/preheaders, and detects simple induction variables (init, step, bound) with trip-count computation.

### `algebraic.rs` — Algebraic identity simplification
Scans all `Binary` instructions and replaces them with `Copy` when a mathematical identity applies. Patterns include:

**Additive**: `x + 0 → x`, `x - 0 → x`, `0 + x → x`, `x - x → 0`
**Multiplicative**: `x * 0 → 0`, `x * 1 → x`, `1 * x → x`, `x * -1 → -x`, `x / 1 → x`, `x / -1 → -x`, `x / x → 1`, `x % 1 → 0`
**Bitwise**: `x & 0 → 0`, `x & -1 → x`, `x | 0 → x`, `x | -1 → -1`, `x ^ 0 → x`, `x ^ x → 0`, `x << 0 → x`, `x >> 0 → x`
**Comparison**: `x == x → 1`, `x != x → 0`, `x <= x → 1`, `x >= x → 1`, `x < x → 0`, `x > x → 0`
**Normalization**: comparisons with constant on the left are flipped to constant-on-right (`5 < x → x > 5`)

### `strength.rs` — Strength reduction
Replaces power-of-two arithmetic with bitwise equivalents:
- `x * 2^k → x << k`
- `x / 2^k → x >> k`
- `x % 2^k → x & (2^k - 1)`

Uses `is_power_of_two()` and `log2()` from `utils.rs`.

### `propagation.rs` — Copy propagation
Collects all `Copy` instructions into a map, transitively resolves chains (`x = y`, `y = z` → use `z` everywhere) with cycle detection, then rewrites all operand references across instructions and terminators — including `FloatBinary` and `FloatUnary`. Dead copies whose destinations are unused are removed.

### `load_forwarding.rs` — Load forwarding
Within each basic block, tracks the last value stored to each address. When a `Load` reads from an address that was just written, the load is replaced with a `Copy` of the stored value. The tracking map is cleared on function calls and stores to unknown addresses.

### `cse.rs` — Common subexpression elimination
Within each basic block, hashes `Binary` instructions by a canonical `(op, left, right)` key (with operand reordering for commutative ops). Duplicates are replaced with a `Copy` of the first result. The expression map resets at block boundaries to prevent invalid cross-block reuse.

### `folding.rs` — Constant folding and DCE
Runs a fixpoint loop (up to 10 iterations) interleaved with dead code elimination. Maintains a per-block constant map; when both operands of a `Binary` resolve to known constants, the result is evaluated at compile time. `Copy` of a constant propagates the value. `CondBr` with a constant condition is folded into `Br`. Covers all integer operators except `Assign` and logical short-circuit.

### `dce.rs` — Dead code elimination
Computes the set of used `VarId`s across all instructions and terminators. Pure instructions (`Binary`, `Unary`, `Copy`, `Cast`, `Load`, `GEP`, `Phi`) whose destination is unused are removed. Side-effecting instructions (`Call`, `Store`, `InlineAsm`, `Alloca`, variadic ops) are always retained.

### `cfg_simplify.rs` — CFG simplification
Iterates three sub-passes to a fixpoint:
1. **Block merging** — when A's only successor is B, and B's only predecessor is A (and B is not a goto target or phi-bearing), A absorbs B's instructions and terminator
2. **Empty block removal** — blocks with no instructions and an unconditional branch are bypassed; all incoming edges are redirected to the target (with transitive closure and cycle detection)
3. **Dead block elimination** — removes unreachable blocks and folds constant-condition branches (`CondBr` where the condition is a known constant) into unconditional `Br`

Merged blocks are tombstoned with `Unreachable` to preserve `BlockId` indexing. Phi operands and terminators referencing removed blocks are updated.

### `loop_interchange.rs` — Loop interchange for cache locality
Swaps the iteration order of perfectly nested loops to improve cache stride patterns. Counts GEP index references to each induction variable in the innermost loop body; if the outer IV appears in more GEP indices (indicating stride-N access), the pass swaps the IV bounds, init values, and step values between the two loop headers to convert column-major access into row-major.

### `licm.rs` — Loop-invariant code motion
Hoists instructions whose operands are all defined outside the loop into the loop's preheader block using a fixed-point iteration (hoisting one instruction may enable further hoisting). Conservatively avoids hoisting stores, calls, phi nodes, and loads when the loop body contains any memory-writing instructions.

### `prefetch.rs` — Software prefetch insertion
Inserts software prefetch hints (`prefetcht0`) for induction-variable-indexed array accesses inside loops. For each qualifying load, emits a GEP + inline-assembly prefetch targeting 16 elements ahead. Only activates when the loop has a known induction variable and trip count ≥ 64, avoiding overhead for small loops.

### `vectorize.rs` — Auto-vectorization (SSE2/AVX2)
Transforms scalar loops into SIMD operations. Analyzes loop bodies for consecutive IV-indexed loads/stores and arithmetic without complex control flow or function calls, then generates a vectorized loop body (with SIMD Load/Store/Add/Sub/Mul IR instructions and VF-stride induction variable) plus a scalar remainder loop for leftover iterations. Selects 4-wide (SSE2) or 8-wide (AVX2) depending on detected hardware support.

### `block_layout.rs` — Basic block reordering
Reorders basic blocks for instruction cache locality. Uses a modified BFS/DFS that prioritizes placing loop body blocks immediately after loop headers (keeping hot loops tight in memory) and deferring cold exit paths, reducing I-cache misses along the most likely execution path.

### `utils.rs`
Shared helpers: `is_power_of_two(n: i64) -> bool` and `log2(n: i64) -> i64`, both `#[inline]`.
