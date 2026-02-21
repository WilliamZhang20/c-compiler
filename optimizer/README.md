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
| 7 | Constant folding + DCE | `folding.rs` | Evaluates compile-time constants; removes dead code |
| 8 | Phi removal | `ir` crate | Lowers phi nodes into copies at predecessor block ends |
| 9 | CFG simplification | `cfg_simplify.rs` | Merges blocks; bypasses empty blocks |

The pipeline runs a single pass (multi-pass iteration was found to cause codegen issues with float function pointers).

## Source files

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
Iterates two sub-passes to a fixpoint:
1. **Block merging** — when A's only successor is B, and B's only predecessor is A (and B is not a goto target or phi-bearing), A absorbs B's instructions and terminator
2. **Empty block removal** — blocks with no instructions and an unconditional branch are bypassed; all incoming edges are redirected to the target (with transitive closure and cycle detection)

Merged blocks are tombstoned with `Unreachable` to preserve `BlockId` indexing. Phi operands and terminators referencing removed blocks are updated.

### `utils.rs`
Shared helpers: `is_power_of_two(n: i64) -> bool` and `log2(n: i64) -> i64`, both `#[inline]`.
