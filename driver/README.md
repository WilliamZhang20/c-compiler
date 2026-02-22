# Driver

The **Driver** is the compiler's CLI entry point. It orchestrates the full compilation pipeline (preprocess → lex → parse → semantic analysis → IR lowering → optimization → codegen → assemble/link) and provides flags to stop at any intermediate stage.

## Usage

```bash
# Full compilation (produces executable)
cargo run -- hello_world.c

# Custom output name
cargo run -- hello_world.c -o my_program

# Compile to object file only (.o, no link)
cargo run -- hello_world.c -c

# Emit assembly only (.s file, no assemble/link)
cargo run -- hello_world.c -S

# Preprocessor flags (forwarded to gcc -E)
cargo run -- hello_world.c -DNDEBUG -DMAX=100 -I/usr/local/include
cargo run -- hello_world.c --include config.h

# Freestanding / no-stdlib compilation
cargo run -- kernel.c --nostdlib --ffreestanding

# Stop after lexing (prints tokens to stdout)
cargo run -- hello_world.c --lex

# Stop after parsing (prints AST to stdout)
cargo run -- hello_world.c --parse

# Stop after codegen (prints IR to stdout, no .s file)
cargo run -- hello_world.c --codegen

# Keep intermediate files (.i preprocessed, .s assembly)
cargo run -- hello_world.c --keep-intermediates

# Debug logging (prints each pipeline stage to stderr + debug_driver.log)
cargo run -- hello_world.c --debug

# Multiple source files
cargo run -- file1.c file2.c -o output
```

## How it works

1. **Preprocessing** — invokes `gcc -E -P -Iinclude` on each input file, producing a `.i` file with all `#include` and `#define` directives expanded.
2. **Lexing** — `lexer::lex()` tokenizes the preprocessed source.
3. **Parsing** — `parser::parse_tokens()` builds the AST. Global variable names are deduplicated (handles `extern` forward declarations).
4. **Semantic analysis** — `SemanticAnalyzer::analyze()` validates the AST.
5. **IR lowering** — `Lowerer::lower_program()` translates AST to SSA-form IR.
6. **Optimization** — `optimizer::optimize()` runs the full pass pipeline.
7. **Code generation** — `Codegen::gen_program()` emits x86-64 assembly text, written to a `.s` file.
8. **Linking** — invokes `gcc` to assemble and link all `.s` files into the final executable.

At any step, `--lex`, `--parse`, `--codegen`, or `-S` will stop the pipeline and output the intermediate result.

## Platform detection

The driver uses `model::Platform::host()` to auto-detect the OS at compile time:
- **Linux**: no executable extension, System V calling convention
- **Windows**: `.exe` extension, `-mconsole` linker flag, Windows x64 convention

## CLI implementation

Built with [clap](https://docs.rs/clap) for argument parsing. The `Args` struct defines all flags with derive macros. Debug logging uses an `OnceLock<bool>` + custom `log!` macro that writes to both stderr and `debug_driver.log`.

## Source files

### `src/main.rs`
The entire driver is a single file (~345 lines). Contains `main()`, `Args` struct, `preprocess()` (GCC invocation with `-D`/`-U`/`-I`/`--include` forwarding), `assemble()` (GCC `.s` → `.o`), and `run_linker()` (GCC invocation for link, with `--nostdlib`/`--ffreestanding` support). Intermediate `.i` and `.s` files are cleaned up unless `--keep-intermediates` or `-S` is specified.

### `tests/integration_tests.rs`
The integration test harness. `run_all_c_tests()` discovers all `.c` files in `testing/`, compiles each one using the driver binary, runs the resulting executable, and asserts the exit code matches the `// EXPECT: <exit_code>` annotation in the first line of the source file. Currently exercises 146 test programs covering the full feature set.
