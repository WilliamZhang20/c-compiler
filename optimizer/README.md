# Optimizer

The **Optimizer** runs a fixed sequence of transformation passes over the SSA-form IR, improving code efficiency without changing observable behavior.

**Public API**:

- `optimizer::optimize(program: IRProgram) -> IRProgram` — default pipeline (no profile)
- `optimizer::optimize_with_options(program, simd_level, profile: Option<&ProfileData>) -> IRProgram` — same pipeline; when `profile` is `Some`, runs **profile-guided block layout** after pass 14

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
| 11 | Auto-vectorization | `vectorize.rs`, `polyhedral.rs`, `mem_dependence.rs` | Converts scalar loops to SIMD (SSE2/AVX2), including gather/scatter |
| 12 | Phi removal | `ir` crate | Lowers phi nodes into copies at predecessor block ends |
| 13 | CFG simplification | `cfg_simplify.rs` | Merges blocks; removes dead blocks; bypasses empty blocks |
| 14 | Block layout | `block_layout.rs` | Reorders blocks for instruction cache locality |
| 15 | Profile layout (optional) | `profile.rs` | When `-fprofile-use` is active, reorders blocks using recorded edge counts |

The pipeline runs a single pass (multi-pass iteration was found to cause codegen issues with float function pointers).

## Profile-guided optimization (PGO)

Built-in instrumentation — **no external profiling libraries**.

| Driver flag | Effect |
|---|---|
| `-fprofile-generate` | Codegen emits `inc qword ptr __profc_<func>_<block>` counters in `.bss` |
| `-fprofile-use=FILE` | Driver loads text profile (`func:block count` lines) and passes to `optimize_with_options` |

Profile data format (one entry per line):

```
main:3 12847
main:5 912
```

`profile.rs` parses this file and `apply_profile_layout()` runs after the standard block-layout pass, placing hot successor blocks adjacent to their predecessors.

**Limitation**: counters are not auto-dumped on program exit; profile files must be written manually (or via a future dumper) from runtime counter values.

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
Transforms scalar loops into SIMD operations. For each natural loop with analyzable induction variable and trip count, builds a `VectorizationPlan` (loads, stores, reductions, arithmetic). Legality and profitability run before IR rewrite:

1. **`polyhedral::allows_vectorization`** — for nested loops, requires a perfect affine nest and inner-only memory indexing (outer IV must not appear in inner GEP indices).
2. **`memory_dependence_ok`** (`mem_dependence.rs`) — no cross-chunk dependence between vectorized load/store sites; strided indices use widened spans (`offset .. offset + scale*(vf-1)`); gather/scatter use per-lane index ranges.
3. **`is_vectorization_profitable`** — trip count and memory-op count thresholds.

**Memory access modes** (`MemAccessMode`):

- **Packed** — unit stride (`scale == 1`): GEP + `Simd::Load` / `Simd::Store`.
- **GatherScatter** — strided affine index with power-of-two scale (e.g. `2*i`): `Simd::IndexSeq` then `Simd::Gather` / `Simd::Scatter`.
- **Indexed** — `a[idx[i]]` where `idx[i]` is a load from an index array at the loop IV: vector load of indices, then gather/scatter on the data array.

Also supports vectorized bitwise ops, masked tail epilogues (`LaneMask`, `Blend`), and horizontal reductions. Emits a vectorized loop (IV += VF) plus scalar remainder. Width: 4 (SSE2) or 8 (AVX2) from `SimdLevel::detect()`.

### `polyhedral.rs` — Affine nest checks (vectorization gate)
Lightweight polyhedral-style analysis (not full ISL/Polly). **Aggressive policy:**

- **Innermost loops** — no nest constraints; legality is delegated to `mem_dependence`.
- **Nested loops** — outer-only blocks may use loads and address arithmetic (`Mul`, shifts, bitwise ops); stores in outer-only blocks still disqualify a perfect nest.
- **Outer-loop vectorization** — when widening the parent IV, outer-only GEPs must not use the child IV in their index.

`prepare_affine_nests` walks nests for validation; `allows_vectorization` gates `vectorize_function`.

### `mem_dependence.rs` — Vectorization dependence testing
Tracks memory accesses with linear `IndexPattern` (`scale * iv + offset`). Computes per-chunk index spans for dependence tests between loads and stores at vector width `vf`, including non-unit stride and gather/scatter lanes. Rejects **reduction-style** patterns: invariant-index store (`scale == 0`) together with IV-strided loads (e.g. `c[i][j] += a[i][k]` with IV `k`). Used by `vectorize.rs` before applying a plan.

### `block_layout.rs` — Basic block reordering
Reorders basic blocks for instruction cache locality. Uses a modified BFS/DFS that prioritizes placing loop body blocks immediately after loop headers (keeping hot loops tight in memory) and deferring cold exit paths, reducing I-cache misses along the most likely execution path. Honors `BranchHint` from `__builtin_expect`.

### `profile.rs` — Profile-guided block layout
Parses the text profile format, maps `func:block` keys to IR `BlockId`s, and reorders blocks so frequently executed edges stay contiguous. Invoked only through `optimize_with_options()` when the driver passes `-fprofile-use=FILE`.

### `utils.rs`
Shared helpers: `is_power_of_two(n: i64) -> bool` and `log2(n: i64) -> i64`, both `#[inline]`.
