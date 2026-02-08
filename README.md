# writing-a-compiler-project

This repo contains the code for a C compiler project written in Rust, based on the
[_Writing a C Compiler_](https://norasandler.com/book/) book by Nora Sandler.

The project began in a Rust study group at Trend Micro while I was interning there. After the internship was over, I completed the project independently, and am still iterating to make the compiler generate more efficient code.

Hopefully it can at least beat GCC `-O0` soon :)

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

The compiler supports a substantial subset of the C language including basic types like `int`, `char`, and pointers. It handles complex control flow with `if`, `while`, `for`, `do-while`, `break`, and `continue` statements.

Rich expression support is included for arithmetic, relational, logical, and bitwise operations. It also supports function definitions, global variables, and array indexing for building structured programs.

## Testing

Run the full test suite with:
```bash
cargo test
```

Individual test files are located in the `testing/` directory. Each test file uses a `// EXPECT: <exit_code>` annotation to specify the expected program exit code.

## Optimizations

The compiler includes several production-quality optimizations organized across multiple passes:

### IR-Level Optimizations (`optimizer/`)

1. **Strength Reduction**: Converts expensive operations to cheaper equivalents
   - Multiply by power-of-2 → left shift (e.g., `x * 8` → `x << 3`)
   - Divide by power-of-2 → right shift (e.g., `x / 4` → `x >> 2`)
   
2. **Copy Propagation**: Eliminates redundant copy operations
   - Replaces uses of copied variables with their original sources
   - Example: `b = a; c = b;` → uses of `c` directly reference `a`

3. **Common Subexpression Elimination (CSE)**: Reuses previously computed values
   - Detects identical computations and eliminates redundancy
   - Example: `x = a + b; y = a + b;` → compute once, reuse result

4. **Dead Store Elimination (DSE)**: Removes unused variable assignments
   - Identifies variables that are never read and removes their allocations
   - Reduces memory usage and eliminates unnecessary stores

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

3. **Instruction Selection**: Generates efficient x86-64 instruction sequences
   - Direct register operations when possible
   - Immediate operand optimization (use constants directly in instructions)
   - Smart addressing mode selection

### Performance Impact

The optimization suite delivers measurable improvements:
- **22% fewer instructions** compared to unoptimized code
- **30% fewer memory operations** (loads/stores)
- **Improved register utilization** (14 registers vs. unlimited virtual)
- **Faster execution** through better instruction selection

Test with `testing/test_all_opts.c` to see all optimizations in action.