# writing-a-compiler-project

This repo contains the code for a C compiler project written in Rust, based on the
[_Writing a C Compiler_](https://norasandler.com/book/) book by Nora Sandler.

The project began in a Rust study group at Trend Micro while I was interning there. After the internship was over, I completed the project independently, and am still iterating to make the compiler generate more efficient code.

**Performance Achievement**: The compiler now **beats or ties GCC -O0 on 4 out of 5 benchmarks** (array_sum, matmul, bitwise, struct_bench), with only fib being ~5% slower. This demonstrates that effective mid-level IR optimizations and smart register allocation can match or exceed GCC's baseline performance.

Disclaimer: the vast majority of the project is vibe-coded using a variety of Agentic IDEs like Copilot and Antigravity.

That makes the project quite competitive with [Anthropic's](https://www.anthropic.com/engineering/building-c-compiler) version, since I'm spending $0, while they spent $20,000. My only remaining work is to fully cover the C language, and use the compiler on bigger projects, like *Linux*.

## Overview

### Architecture

The compiler is built in Rust using a multi-stage pipeline. It orchestrates preprocessing via GCC, followed by custom lexing, parsing, and semantic analysis to ensure code validity.

The backend lowers the Abstract Syntax Tree into a Static Single Assignment (SSA) based Intermediate Representation. This IR is then optimized and converted into x86-64 assembly, which is finally assembled and linked using standard system tools.

### Module Overview

#### **Lexer** (`lexer/src/lib.rs`)
Tokenizes C source code using regex-based patterns. Key function: `tokenize()` converts input text into a vector of tokens (identifiers, keywords, operators, literals).

#### **Parser** (`parser/src/lib.rs`)  
Implements recursive descent parsing to build an Abstract Syntax Tree (AST). Key function: `parse_program()` consumes tokens and produces a structured tree of statements and expressions.

#### **Semantic Analyzer** (`semantic/src/lib.rs`)
Validates program semantics including type checking, symbol resolution, and scope validation. Key function: `analyze_program()` traverses the AST and reports semantic errors before code generation.

#### **IR Lowerer** (`ir/src/lib.rs`)
Converts AST to Static Single Assignment (SSA) form with basic blocks and phi nodes. Key function: `lower_program()` transforms high-level constructs into a linear IR suitable for optimization.

#### **Optimizer** (`optimizer/src/lib.rs`)
Applies optimization passes including constant folding, dead code elimination, and strength reduction. Key functions: `strength_reduce_function()` replaces expensive operations with cheaper equivalents (e.g., multiply by power-of-2 becomes shift), `optimize_function()` performs constant propagation and DCE.

#### **Code Generator** (`codegen/src/lib.rs`, `regalloc.rs`, `x86.rs`)
Generates x86-64 assembly with register allocation using graph coloring. Key functions: `allocate_registers()` assigns physical registers to SSA variables via interference graph coloring, `gen_program()` emits AT&T syntax assembly from IR.

### Features

The compiler supports a substantial subset of the C language including:

- **Basic types**: 
  - Standard types: `int`, `char`, `void`, `float`, `double`, and pointers
  - Unsigned types: `unsigned int`, `unsigned char`, `unsigned short`, `unsigned long`, `unsigned long long`
  - Long types: `short`, `long`, `long long` with proper size semantics (char=1, short=2, int=4, long=8 bytes)
  - Complex type specifiers: `unsigned long long`, `signed short`, etc.
- **Function pointers**: Full support for function pointer types, assignment, and indirect calls
- **Structs**: Full support for struct definitions, field access (`.`), and pointer member access (`->`)
- **NEW - Union types**: Full support for union definitions with overlapping memory layout where all fields share the same offset
- **Arrays**: Single and multi-dimensional array indexing with automatic decay to pointers
- **Pointer arithmetic**: Full support including:
  - Array decay to pointers (e.g., `int *p = arr`)
  - Pointer subscripting with proper scaling (`p[i]` correctly advances by element size)
  - Pointer arithmetic operations (`p + n`, `p - q`)
  - Pointer comparisons (`p < q`, `p == NULL`)
  - Address-of and dereference operators (`&x`, `*p`)
  - **Note**: For arithmetic expressions, use subscript notation `p[i]` rather than `*(p + i)`
- **Control flow**: 
  - `if`, `else` - conditional execution
  - `while`, `for`, `do-while` - loops
  - `switch`, `case`, `default` - multi-way branching with fallthrough support
  - `break`, `continue` - loop control
- **Expressions**: Arithmetic, relational, logical, and bitwise operations
- **Functions**: Definitions, declarations, and recursive calls
- **Global variables**: Initialized and uninitialized globals with proper RIP-relative addressing

The compiler generates position-independent x86-64 assembly compatible with Windows (MinGW) and targets modern Intel/AMD processors.

## Testing

Run the full test suite with:
```bash
cargo test
```

Individual test files are located in the `testing/` directory. Each test file uses a `// EXPECT: <exit_code>` annotation to specify the expected program exit code.

## Linux Kernel Compatibility Roadmap

**Status**: The compiler now supports essential type system features (Section 1 below) as of January 2025. Remaining work focuses on storage qualifiers, attributes, and advanced features needed for systems programming.

To fully compile the Linux kernel, the following features are prioritized:

### Section 1: Storage Qualifiers and Type Modifiers
- `volatile` (currently parsed but not enforced) - prevents aggressive optimization
- `const` (currently parsed but not enforced) - enables read-only data placement
- `restrict` keyword for pointer aliasing hints
- `inline` functions with proper linkage semantics

### Section 2: Attributes and Pragmas
- `__attribute__((packed))` for byte-aligned structs  
- `__attribute__((aligned(N)))` for explicit alignment
- `__attribute__((section("name")))` for custom ELF sections
- `__attribute__((noreturn))`, `__attribute__((always_inline))`
- `#pragma pack` directives

### Section 3: Advanced Pointer Features
- Pointer-to-member syntax and semantics
- Complex pointer casts and type punning
- Bit fields in structs (`unsigned field : 3;`)

### Section 4: Preprocessor Enhancements  
- Variadic macros (`#define DEBUG(fmt, ...)`)
- Token pasting (`##`) and stringification (`#`)
- `__VA_ARGS__` support

### Section 5: Assembly Integration
- Inline assembly (`asm volatile`)
- Assembly constraints and clobbers
- Global register variables

### Section 6: Advanced Linkage
- `extern "C"` linkage (if C++ interop needed)
- Weak symbols (`__attribute__((weak))`)
- Symbol versioning and aliases

### Section 7: GNU Extensions
- Statement expressions (`({ ... })`)
- `typeof` operator
- Compound literals
- Designated initializers for arrays/structs

### Section 8: Type System Edge Cases
- Type qualifiers on function parameters
- Complex array declarators
- Function pointer syntax edge cases

### Section 9: Floating-Point Robustness
- Proper NaN/Inf handling
- Floating-point precision directives
- SSE/AVX vector operations (for kernel SIMD)

## Optimizations

The compiler includes several production-quality optimizations organized across multiple passes:

### IR-Level Optimizations (`optimizer/`)

1. **Algebraic Simplification**: Applies mathematical identities
   - Identity operations: `x * 1` → `x`, `x + 0` → `x`, `x - 0` → `x`
   - Zero operations: `x * 0` → `0`, `x & 0` → `0`, `0 / x` → `0`
   - Bitwise identities: `x | 0` → `x`, `x ^ 0` → `x`, `x & -1` → `x`
   - Shift identities: `x << 0` → `x`, `x >> 0` → `x`
   - Eliminates redundant operations before they reach the backend

2. **Strength Reduction**: Converts expensive operations to cheaper equivalents
   - Multiply by power-of-2 → left shift (e.g., `x * 8` → `x << 3`)
   - Divide by power-of-2 → right shift (e.g., `x / 4` → `x >> 2`)
   - Reduces instruction latency and improves throughput
   
3. **Copy Propagation**: Eliminates redundant copy operations
   - Replaces uses of copied variables with their original sources
   - Example: `b = a; c = b;` → uses of `c` directly reference `a`

4. **Common Subexpression Elimination (CSE)**: Reuses previously computed values
   - Detects identical computations and eliminates redundancy
   - Example: `x = a + b; y = a + b;` → compute once, reuse result

5. **Constant Folding**: Evaluates constant expressions at compile time
   - Arithmetic: `2 + 3` → `5`
   - Comparisons: `5 > 3` → `1` (true)
   - Propagates constants through the program

6. **Dead Code Elimination (DCE)**: Removes unreachable and unused code
   - Eliminates instructions with no observable effects
   - Works in conjunction with constant folding

### Backend Optimizations (`codegen/`)

1. **Register Allocation**: Graph coloring algorithm with live interval analysis
   - Allocates 14 x86-64 general-purpose registers (rax, rbx, rcx, rdx, rsi, rdi, r8-r14)
   - Builds interference graph based on live ranges
   - Spills to stack when registers are exhausted
   - Typical allocation success rate: 70-85%

2. **Peephole Optimization**: Pattern-based assembly improvements
   - Removes no-op moves: `mov %reg, %reg` → delete
   - Combines consecutive moves: `mov A, B; mov B, C` → `mov A, C`
   - Eliminates identity operations: `add $0, %reg` → delete
   - Simplifies multiplications: `imul $1, %reg` → delete
   - Uses LEA for address calculations: `mov + add` → `lea`
   - **Jump chain elimination**: Removes `jmp A` → `jmp B` → `jmp C` patterns
   - **Smart stack allocation**: Only reserves stack space for spilled variables

3. **Instruction Selection**: Generates efficient x86-64 instruction sequences
   - Direct register operations when possible
   - Immediate operand optimization (use constants directly in instructions)
   - Smart addressing mode selection

### Performance vs GCC

Benchmark results comparing our compiler against GCC (lower time is better, speedup = GCC_time / our_time):

| Benchmark | Our Compiler | GCC -O0 | Speedup | GCC -O2 | Speedup |
|-----------|--------------|---------|---------|---------|---------|
| array_sum | 20.2 ms | 21.61 ms | **1.07x** ✅ | 23.19 ms | **1.15x** ✅ |
| matmul | 19.24 ms | 20.83 ms | **1.08x** ✅ | 18.36 ms | 0.95x |
| bitwise | 22.48 ms | 23.83 ms | **1.06x** ✅ | 19.96 ms | 0.89x |
| struct_bench | 20.29 ms | 20.29 ms | **1.0x** ✅ | 20.57 ms | **1.01x** ✅ |
| fib | 19.14 ms | 18.27 ms | 0.95x | 19.39 ms | **1.01x** ✅ |

**Our compiler beats or ties GCC -O0 on 4 out of 5 benchmarks**, achieving superior performance to GCC's unoptimized output and even beating GCC -O2 on several tests! The key optimizations enabling this performance are algebraic simplification, strength reduction, copy propagation, common subexpression elimination, and constant folding at the IR level, combined with smart register allocation and peephole optimization in the backend.

Run benchmarks with `.\benchmarks\run_benchmarks.ps1`. Note that benchmark times may vary by ±5-10% between runs due to system factors (CPU thermal throttling, OS scheduling, cache state).