# C Compiler in Rust

A C compiler targeting x86-64 Linux and Windows, written in Rust. It handles the full pipeline from tokenization through register allocation and assembly emission, producing native executables via GCC as the assembler/linker.

Originally based on [_Writing a C Compiler_](https://norasandler.com/book/) by Nora Sandler, the project has been extended well beyond the book's scope with C99/C11 features, GCC extensions, SSA-based optimizations, and graph-coloring register allocation.

**Performance**: The compiler **beats or ties GCC -O0** on all 5 benchmark programs (matrix multiply, fibonacci, array sum, bitwise ops, struct access). This comes from a 14-pass SSA optimization pipeline (constant folding, strength reduction, CSE, copy propagation, DCE, auto-vectorization, LICM, loop interchange, prefetching, block layout) and graph-coloring register allocation that minimizes spills.

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
│  Semantic    │  name resolution, qualifier checks, control flow validation
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
| **semantic** | Name resolution, qualifier enforcement, control flow checks | `SemanticAnalyzer::analyze(program)` |
| **ir** | AST → SSA IR lowering with Braun et al. phi construction | `Lowerer::lower_program(program)` |
| **optimizer** | 14-pass optimization pipeline over SSA IR | `optimizer::optimize(ir_program)` |
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
- `break`, `continue`, `goto`/labels
- `return` with optional expression
- Block scoping with `{ }`
- Inline assembly (`asm`/`__asm__`) with operand constraints

### Declarations and Attributes
- `static`, `extern`, `inline`, `register`, `const`, `volatile`, `restrict`
- `_Noreturn` / `noreturn`
- `typedef` for type aliases
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
11. **Auto-vectorization** — transforms scalar loops into SIMD operations (SSE2 4-wide / AVX2 8-wide) for consecutive IV-indexed loads, stores, and arithmetic; generates a vectorized body plus scalar remainder loop
12. **Phi removal** — deconstructs phi nodes into copies at predecessor block ends
13. **CFG simplification** — merge single-successor/single-predecessor block pairs, bypass empty blocks, eliminate dead blocks, fold constant branches
14. **Block layout** — reorders basic blocks for I-cache locality, keeping hot loop bodies tight and deferring cold exit paths

## Testing

```bash
# Run everything: unit tests + integration tests
cargo test

# Run only unit tests (fast)
cargo test --lib

# Run only integration tests (compiles 165 C programs)
cargo test --test integration_tests
```

The integration test harness (`driver/tests/integration_tests.rs`) discovers all `.c` files in `testing/`, compiles each one using the compiler, runs the resulting executable, and asserts the exit code matches the `// EXPECT: <exit_code>` annotation in the source file.

**Current status**: 165 integration test programs (160 with EXPECT annotations), all passing. 268 unit tests across all crates.

Run `./coverage.sh` for line-level coverage analysis via `cargo-tarpaulin` (use `--quick` to reuse the last report).

### Auto-Vectorization

The auto-vectorizer (pass 11) analyzes loop bodies for consecutive IV-indexed loads/stores with compatible arithmetic (Add, Sub, Mul). When a loop qualifies:
- Detects hardware SIMD support (SSE2 → 4-wide, AVX2 → 8-wide)
- Generates a vectorized loop body using SIMD IR instructions (`SimdLoad`, `SimdStore`, `SimdAdd`, etc.)
- Adjusts the induction variable stride to the vector width
- Emits a scalar remainder loop for iterations not divisible by the vector factor

### Cache Locality Optimizations

Three passes target memory hierarchy performance:
- **Loop interchange** (pass 8) — detects nested loops where reordering improves spatial locality. Counts which induction variable dominates array index expressions; if the outer IV has more references, swapping gives sequential access.
- **Software prefetch** (pass 10) — for large loops (trip count ≥ 64) with IV-indexed array loads, inserts `prefetcht0` hints to bring cache lines into L1 before they're needed, hiding memory latency.
- **Block layout** (pass 14) — reorders the CFG so hot loop bodies are contiguous in the instruction stream, keeping tight loops within a single I-cache line and deferring cold error/exit paths.

## Benchmarks

Five benchmark programs in `benchmarks/` compare this compiler against GCC -O0:

| Benchmark | Description |
|---|---|
| `fib.c` | Recursive Fibonacci |
| `array_sum.c` | Array summation |
| `matmul.c` | Matrix multiplication |
| `bitwise.c` | Bitwise operations |
| `struct_bench.c` | Struct field access patterns |

Run benchmarks with `benchmarks/run_benchmarks.sh`. Results are in `benchmarks/results_linux.md`.