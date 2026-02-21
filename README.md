# C Compiler in Rust

A C compiler targeting x86-64 Linux and Windows, written in Rust. It handles the full pipeline from tokenization through register allocation and assembly emission, producing native executables via GCC as the assembler/linker.

Originally based on [_Writing a C Compiler_](https://norasandler.com/book/) by Nora Sandler, the project has been extended well beyond the book's scope with C99/C11 features, GCC extensions, SSA-based optimizations, and graph-coloring register allocation.

**Performance**: The compiler **beats or ties GCC -O0** on all 5 benchmark programs (matrix multiply, fibonacci, array sum, bitwise ops, struct access). This comes from SSA-level optimizations (constant folding, strength reduction, CSE, copy propagation, DCE) and hint-driven register allocation that minimizes spills.

## Building and Running

**Prerequisites**: Rust toolchain (`cargo`), GCC (used for preprocessing and linking).

```bash
# Build the compiler
cargo build --bin driver --release

# Compile a C file to an executable
./target/release/driver hello_world.c

# Emit assembly only (no assemble/link)
./target/release/driver hello_world.c -S

# See tokens
./target/release/driver hello_world.c --lex

# See AST
./target/release/driver hello_world.c --parse

# Keep intermediate files (.i preprocessed, .s assembly)
./target/release/driver hello_world.c --keep-intermediates

# Custom output name
./target/release/driver hello_world.c -o my_program

# Enable debug logging
./target/release/driver hello_world.c --debug
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
│  Optimizer   │  mem2reg → algebraic → strength → copy prop → CSE
│              │  → constant fold/DCE → phi removal → CFG simplify
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
| **optimizer** | 8-pass optimization pipeline over SSA IR | `optimizer::optimize(ir_program)` |
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
- Integer literal suffixes: `U`, `L`, `UL`, `LL`, `ULL`

### Pointer Arithmetic
- Array-to-pointer decay
- Pointer subscripting with element-size scaling (`p[i]`)
- Pointer addition/subtraction with proper scaling
- Pointer comparison (`<`, `>`, `==`, `!=`)
- Address-of (`&`) and dereference (`*`)

## Optimization Pipeline

The optimizer runs 8 passes in a fixed sequence:

1. **mem2reg** — promotes `alloca`/`load`/`store` of scalar locals to SSA registers via phi-node insertion
2. **Algebraic simplification** — identity removal (`x+0`, `x*1`, `x&-1`), strength patterns (`x-x→0`, `x^x→0`, `x*-1→-x`, `x/x→1`), comparison normalization
3. **Strength reduction** — `x * 2^k → x << k`, `x / 2^k → x >> k`, `x % 2^k → x & (2^k-1)`
4. **Copy propagation** — transitive resolution of copy chains with dead copy removal
5. **Common subexpression elimination** — per-block hash-based deduplication with commutativity-aware canonicalization
6. **Constant folding + DCE** — fixpoint loop: evaluate compile-time constant operations, fold constant branches, then remove dead instructions
7. **Phi removal** — deconstructs phi nodes into copies at predecessor block ends
8. **CFG simplification** — merge single-successor/single-predecessor block pairs, bypass empty blocks

## Testing

```bash
# Run everything: unit tests + integration tests
cargo test

# Run only unit tests (fast)
cargo test --lib

# Run only integration tests (compiles 142 C programs)
cargo test --test integration_tests
```

The integration test harness (`driver/tests/integration_tests.rs`) discovers all `.c` files in `testing/`, compiles each one using the compiler, runs the resulting executable, and asserts the exit code matches the `// EXPECT: <exit_code>` annotation in the source file.

**Current status**: 142 integration test programs, all passing. 34 unit tests across all crates.

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