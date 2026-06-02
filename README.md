# C Compiler in Rust

A C compiler targeting x86-64 Linux and Windows, written in Rust. It handles the full pipeline from tokenization through register allocation and assembly emission, producing native executables via GCC as the assembler/linker.

Originally based on [_Writing a C Compiler_](https://norasandler.com/book/) by Nora Sandler, the project has been extended well beyond the book's scope with C99/C11 features, GCC extensions, SSA-based optimizations, and graph-coloring register allocation.

**Performance**: The compiler targets competitive performance against **GCC -O0**, **GCC -O2**, and **GCC -O3** on benchmark programs (matrix multiply, fibonacci, array sum, bitwise ops, struct access), using a 14-pass SSA optimization pipeline (constant folding, strength reduction, CSE, copy propagation, DCE, auto-vectorization, LICM, loop interchange, prefetching, block layout) and graph-coloring register allocation.

## Building and Running

**Prerequisites**: Rust toolchain (`cargo`), GCC (used for preprocessing and linking).

```bash
# Build the compiler
cargo build --bin driver --release

# Compile a C file to an executable
./target/release/driver hello_world.c

# Compile to object file only (no link)
./target/release/driver -c hello_world.c

# Emit assembly only (no assemble/link)
./target/release/driver hello_world.c -S

# See tokens
./target/release/driver hello_world.c --lex

# See AST
./target/release/driver hello_world.c --parse

# Custom output name
./target/release/driver hello_world.c -o my_program

# Preprocessor flags (forwarded to gcc -E)
./target/release/driver -DNDEBUG -DMAX=100 -I/usr/local/include hello_world.c

# Force-include a header
./target/release/driver --include config.h hello_world.c

# Freestanding / no standard library
./target/release/driver --nostdlib --ffreestanding kernel.c

# Position-independent code (shared objects / modules)
./target/release/driver -fPIC -shared -c module.c
./target/release/driver -fPIE -fpie -o prog app.c

# Profile-guided optimization (built-in; no external profiler libs)
./target/release/driver -fprofile-generate -o prog app.c
./prog   # run workload; counters live in __profc_* globals
./target/release/driver -fprofile-use=default.prof -o prog app.c

# Machine / kernel flags
./target/release/driver --mno-red-zone --mno-sse kernel.c

# Keep intermediate files (.i preprocessed, .s assembly)
./target/release/driver hello_world.c --keep-intermediates

# Enable debug logging
./target/release/driver hello_world.c --debug

# Multiple source files
./target/release/driver file1.c file2.c -o output
```

On Windows, the same binary works with MinGW GCC. The compiler auto-detects the host platform and adjusts the calling convention (System V vs Windows x64) and executable extension.

## Architecture

```
 C source
    │
    ▼
┌──────────────┐    gcc -E
│ Preprocessor │ ◄──────────  (external)
└──────┬───────┘
       │  preprocessed .i
       ▼
┌──────────────┐
│    Lexer     │  byte-oriented state machine → Vec<Token>
└──────┬───────┘
       ▼
┌──────────────┐
│    Parser    │  recursive descent + precedence climbing → AST
└──────┬───────┘
       ▼
┌──────────────┐
│  Semantic    │  TypeEnv: promotions, assignment/call checks, qualifiers
└──────┬───────┘
       ▼
┌──────────────┐
│ IR Lowerer   │  AST → SSA-form basic-block CFG (Braun et al. algorithm)
└──────┬───────┘
       ▼
┌──────────────┐
│  Optimizer   │  mem2reg → algebraic → strength → copy prop → load fwd
│              │  → CSE → fold/DCE → loop interchange → LICM → prefetch
│              │  → auto-vectorize → phi removal → CFG simplify → layout
└──────┬───────┘
       ▼
┌──────────────┐
│   Codegen    │  graph-coloring regalloc → x86-64 instruction selection
│              │  → peephole optimization → Intel-syntax assembly text
└──────┬───────┘
       │  .s file
       ▼
┌──────────────┐    gcc
│ Assembler/   │ ◄──────────  (external)
│   Linker     │
└──────────────┘
       │
       ▼
   executable
```

## Crate Structure

The workspace is split into 8 crates with clear dependency flow:

| Crate | Purpose | Key entry point |
|---|---|---|
| **model** | Shared AST types: `Token`, `Expr`, `Stmt`, `Type`, `Attribute`, platform config | `use model::*` |
| **lexer** | Tokenization of C source into `Vec<Token>` | `lexer::lex(src)` |
| **parser** | Recursive descent parser producing AST `Program` | `parser::parse_tokens(tokens)` |
| **semantic** | `TypeEnv` type checking: promotions, calls, assignments, qualifiers | `SemanticAnalyzer::analyze(program)` |
| **ir** | AST → SSA IR lowering with Braun et al. phi construction | `Lowerer::lower_program(program)` |
| **optimizer** | 14-pass pipeline + optional PGO block layout | `optimizer::optimize(ir_program)` or `optimize_with_options(..., profile)` |
| **codegen** | x86-64 assembly generation with graph-coloring register allocation | `Codegen::gen_program(ir_program)` |
| **driver** | CLI entry point, orchestrates the full pipeline | `cargo run -- file.c` |

Dependency graph: `driver` → `codegen` → `optimizer` → `ir` → `semantic` → `parser` → `lexer` → `model`.

## Supported C Language Features

### Types
- **Integer types**: `char` (1B), `short` (2B), `int` (4B), `long` (8B), `long long` (8B), all with `signed`/`unsigned` variants
- **Floating-point**: `float` (single), `double` (double precision)
- **Boolean**: `_Bool` / `bool` (C99, 1 byte)
- **Void**, **pointers** (including multi-level), **arrays** (single and multi-dimensional)
- **Structs** with field access (`.`), pointer access (`->`), bit-fields, `__attribute__((packed))`, designated initializers
- **Unions** with overlapping memory layout
- **Enums** with explicit or auto-incremented values
- **Typedefs** and complex declarators
- **Function pointers**: declaration, assignment, indirect calls through FP variables

### Expressions
- Full arithmetic, relational, logical, and bitwise operators
- Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`)
- Pre/post increment/decrement
- Comma operator (left-to-right evaluation, returns last)
- Ternary `?:` with GCC extension for omitted middle operand (`x ?: y`)
- `sizeof(type)`, `sizeof expr`, `_Alignof(type)` / `__alignof__`
- Type casts between integer, float, and pointer types
- Compound literals: `(int[]){1, 2, 3}`
- GNU statement expressions: `({ int x = 1; x + 2; })`
- `_Generic` selection (C11): type-based compile-time dispatch

### Statements
- Variable declarations with initializers (including C99 `for`-init declarations)
- Multi-variable declarations: `int a = 1, b = 2;`
- `if`/`else`, `while`, `do-while`, `for` loops
- `switch`/`case`/`default` with fallthrough
- `break`, `continue`, `goto`/labels, **computed goto** (`goto *expr`; IR `IndirectBr`)
- **Label addresses** (`&&label`) — parsed; codegen emits rodata pointers (parser edge cases remain)
- `return` with optional expression
- Block scoping with `{ }`
- Inline assembly (`asm`/`__asm__`) with operand constraints

### Declarations and Attributes
- `static`, `extern`, `inline`, `register`, `const`, `volatile`, `restrict`
- `_Noreturn` / `noreturn`
- Designated initializers (`.field`, `[index]`, nested `.a.b`, GCC ranges `[lo ... hi]`)
- Function prototypes stored in `Program.prototypes`; **`typedef` definitions** in `Program.typedefs`
- `_Static_assert(expr, "message")` (C11)
- `__attribute__((packed))`, `__attribute__((aligned(N)))`, `__attribute__((section("name")))`
- `__attribute__((noreturn))`, `__attribute__((always_inline))`
- `__attribute__((weak))`, `__attribute__((unused))`
- `__attribute__((constructor))`, `__attribute__((destructor))` — emits `.init_array`/`.fini_array`

### GCC Builtins and Extensions
- `__builtin_expect(expr, val)` — branch prediction hint (transparent passthrough)
- `__builtin_offsetof(type, member)` — compile-time struct field offset
- `__builtin_types_compatible_p(t1, t2)` — compile-time type comparison
- `__builtin_choose_expr(const, e1, e2)` — compile-time conditional
- `__builtin_unreachable()` / `__builtin_trap()` — unreachable code markers
- `__builtin_clz(x)`, `__builtin_ctz(x)`, `__builtin_popcount(x)`, `__builtin_abs(x)` — bit/math intrinsics (compile-time evaluated for constants, inline code for `abs`)
- `typeof(expr)` / `__typeof__(expr)` — type inference
- Multi-character constants: `'ABCD'` packed big-endian
- Integer literal suffixes: `U`, `L`, `UL`, `LL`, `ULL` (tracked as `IntegerSuffix` in the token)
- Octal integer literals: `0777`, `0644`
- Binary integer literals: `0b1010`, `0B11111111` (GCC extension)

### Pointer Arithmetic
- Array-to-pointer decay
- Pointer subscripting with element-size scaling (`p[i]`)
- Pointer addition/subtraction with proper scaling
- Pointer comparison (`<`, `>`, `==`, `!=`)
- Address-of (`&`) and dereference (`*`)

## Optimization Pipeline

The optimizer runs 14 passes in a fixed sequence:

1. **mem2reg** — promotes `alloca`/`load`/`store` of scalar locals to SSA registers via phi-node insertion
2. **Algebraic simplification** — identity removal (`x+0`, `x*1`, `x&-1`), strength patterns (`x-x→0`, `x^x→0`, `x*-1→-x`, `x/x→1`), comparison normalization
3. **Strength reduction** — `x * 2^k → x << k`, `x / 2^k → x >> k`, `x % 2^k → x & (2^k-1)`
4. **Copy propagation** — transitive resolution of copy chains with dead copy removal
5. **Load forwarding** — replaces loads with previously stored values within a basic block
6. **Common subexpression elimination** — per-block hash-based deduplication with commutativity-aware canonicalization
7. **Constant folding + DCE** — fixpoint loop: evaluate compile-time constant operations, fold constant branches, then remove dead instructions
8. **Loop interchange** — swaps nested loop iteration order when the inner loop has stride-N access on the outer induction variable, converting column-major to row-major traversal for cache locality
9. **Loop-invariant code motion (LICM)** — hoists computations whose operands are loop-invariant into the loop preheader using fixed-point iteration
10. **Software prefetch insertion** — emits `prefetcht0` hints for IV-indexed array accesses in loops with trip count ≥ 64, prefetching 16 elements ahead
11. **Auto-vectorization** — transforms scalar loops into SIMD operations (SSE2 4-wide / AVX2 8-wide) for unit-stride, strided (`a[2*i]`), and indexed (`a[idx[i]]`) memory access, with polyhedral-style nest checks and dependence analysis; generates a vectorized body plus scalar remainder loop
12. **Phi removal** — deconstructs phi nodes into copies at predecessor block ends
13. **CFG simplification** — merge single-successor/single-predecessor block pairs, bypass empty blocks, eliminate dead blocks, fold constant branches
14. **Block layout** — reorders basic blocks for I-cache locality, keeping hot loop bodies tight and deferring cold exit paths
15. **Profile layout** (optional, `-fprofile-use`) — reorders blocks using recorded execution counts from a text profile file

## Testing

```bash
# Run everything: unit tests + integration tests
cargo test

# Run only unit tests (fast)
cargo test --lib

# Run only integration tests (compiles 177 C programs)
cargo test --test integration_tests
```

The integration test harness (`driver/tests/integration_tests.rs` and `driver/tests/inprocess_tests.rs`) discovers all `.c` files in `testing/`, compiles each one using the compiler, runs the resulting executable, and asserts the exit code matches the `// EXPECT: <exit_code>` annotation in the source file.

**Current status**: 174 integration test programs in `testing/` (167 run with EXPECT checks, 7 skipped e.g. missing headers), all passing. Unit tests across all crates run via `cargo test`.

Run `./coverage.sh` for line-level coverage analysis via `cargo-tarpaulin` (use `--quick` to reuse the last report).

### Auto-Vectorization

The auto-vectorizer (pass 11, `optimizer/src/vectorize.rs`) analyzes natural loops for memory and arithmetic that can execute in SIMD form. Before widening, `polyhedral.rs` applies an aggressive nest policy (innermost loops are always eligible; nested loops require a near-perfect outer shell). `mem_dependence.rs` proves that vectorized chunks do not introduce cross-iteration conflicts—including rejecting reduction loops with a fixed store slot and IV-strided loads (e.g. matmul inner `k`).

**Access modes**

| Pattern | Example | IR |
|--------|---------|-----|
| Packed | `a[i]`, `b[i]` | `Simd::Load` / `Simd::Store` on a GEP |
| Strided gather/scatter | `a[2*i] = b[2*i]` | `Simd::IndexSeq` + `Gather` / `Scatter` |
| Indexed | `a[idx[i]] = b[idx[i]]` | vector load of `idx[i]`, then `Gather` / `Scatter` |

**When a loop qualifies**

- Trip count and memory traffic meet profitability heuristics; no internal branches or calls in the vectorized body
- Hardware SIMD level sets vector width (SSE2 → 4-wide, AVX2 → 8-wide)
- Vectorized header/body uses `Simd` ops (`Load`, `Store`, `Add`, `Sub`, `Mul`, bitwise ops, `LaneMask`/`Blend` for masked tails, `IndexSeq`, `Gather`, `Scatter`)
- IV advances by VF per vector iteration; a scalar remainder loop handles leftover iterations

**Codegen** (`codegen/src/function.rs`): contiguous ops use `vmovdqu` / `vpaddd`; gathers use AVX2 `vpgatherdd` when available; scatters use a scalar per-lane loop (GNU `as` does not accept `vpscatterdd` in Intel syntax on typical Linux toolchains). Stack arrays use `lea` into `r10` as the gather/scatter base.

**Tests**: `testing/test_vectorize_*.c` (copy, bitwise, masked tail, strided gather, indexed gather, etc.).

### Cache Locality Optimizations

Three passes target memory hierarchy performance:
- **Loop interchange** (pass 8) — detects nested loops where reordering improves spatial locality. Counts which induction variable dominates array index expressions; if the outer IV has more references, swapping gives sequential access.
- **Software prefetch** (pass 10) — for large loops (trip count ≥ 64) with IV-indexed array loads, inserts `prefetcht0` hints to bring cache lines into L1 before they're needed, hiding memory latency.
- **Block layout** (pass 14) — reorders the CFG so hot loop bodies are contiguous in the instruction stream, keeping tight loops within a single I-cache line and deferring cold error/exit paths.

## Benchmarks

Five benchmark programs in `benchmarks/` compare this compiler against **GCC -O0**, **GCC -O2**, and **GCC -O3** (see `benchmarks/run_benchmarks.sh`):

| Benchmark | Description |
|---|---|
| `fib.c` | Iterative Fibonacci (O(n) loop in source) |
| `array_sum.c` | Array summation |
| `matmul.c` | Matrix multiplication |
| `bitwise.c` | Bit manipulation (`__builtin_popcount`) |
| `struct_bench.c` | Struct field access patterns |

Run benchmarks with `benchmarks/run_benchmarks.sh`. Results are in `benchmarks/results_linux.md`.