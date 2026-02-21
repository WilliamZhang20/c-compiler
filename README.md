# writing-a-compiler-project

This repo contains the code for a C compiler project written in Rust, based on the
[_Writing a C Compiler_](https://norasandler.com/book/) book by Nora Sandler.

The project began in a Rust study group at Trend Micro while I was interning there. After the internship was over, I completed the project independently, and am still iterating to make the compiler generate more efficient code.

**Performance Achievement**: The compiler now **beats or ties GCC -O0** on 5 benchmark programs in the `benchmarks folder`. This demonstrates that effective mid-level IR optimizations and smart register allocation can match or exceed GCC's baseline performance.

Disclaimer: the vast majority of the project is vibe-coded using a variety of Agentic IDEs like Copilot and Antigravity.

That makes the project quite competitive with [Anthropic's](https://www.anthropic.com/engineering/building-c-compiler) version, since I'm spending $0, while they spent $20,000. My only remaining work is to fully cover the C language, and use the compiler on bigger projects, like *Linux*.

## Overview

### How to Run

The most exciting part. The compiler now works on both **Windows** and **Linux**! On Windows, run `windows_release.exe` and point to any C file. On Linux, build and run `./linux_release`. The compiler successfully compiles and runs complex programs including the classic `donut.c` spinning donut animation, matching GCC's output exactly. The next milestone is expanding language coverage and using the compiler on larger real-world projects.

To build a new release from the existing files, run the following:
```
cargo build --bin driver --release
```

- On Windows: Copy `target/release/driver.exe` to the home directory
- On Linux: Copy `target/release/driver` to the home directory

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
Generates x86-64 assembly with register allocation using graph coloring. Key functions: `allocate_registers()` assigns physical registers to SSA variables via interference graph coloring, `gen_program()` emits Intel syntax assembly from IR.

### Features

The compiler supports a substantial subset of the C language including:

- **Basic types**: 
  - Standard types: `int`, `char`, `void`, `float`, `double`, and pointers
  - Unsigned types: `unsigned int`, `unsigned char`, `unsigned short`, `unsigned long`, `unsigned long long`
  - Long types: `short`, `long`, `long long` with proper size semantics (char=1, short=2, int=4, long=8 bytes)
  - Complex type specifiers: `unsigned long long`, `signed short`, etc.
- **Function pointers**: Full support for function pointer types, assignment, and indirect calls
- **Structs**: Full support for struct definitions, field access (`.`), and pointer member access (`->`)
- **Union types**: Full support for union definitions with overlapping memory layout where all fields share the same offset
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

The compiler generates position-independent x86-64 assembly compatible with **both Windows (MinGW) and Linux (System V ABI)**, targeting modern Intel/AMD processors.

## Testing

Run the full test suite with:
```bash
cargo test
```

Individual test files are located in the `testing/` directory. Each test file uses a `// EXPECT: <exit_code>` annotation to specify the expected program exit code.