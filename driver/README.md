# Driver

The **Driver** crate is the entry point of the compiler CLI. It coordinates the entire compilation pipeline, orchestrating the execution of the lexer, parser, semantic analyzer, optimizer, and code generator. It also interfaces with external tools like `gcc` for preprocessing and linking.

This component parses command-line arguments provided by the user and determines which actions to perform. It manages input and output files, handles intermediate files (like `.i` preprocessed source or `.s` assembly), and drives the flow of data between the various compiler stages.

## Command Line Arguments

The driver accepts the following arguments:

*   `input_path`: The path to the C source file to compile (required).
*   `--lex` (`-l`): Run only the lexer and print the stream of tokens. Useful for debugging tokenization.
*   `--parse` (`-p`): Run the lexer and parser, then print the AST. Useful for debugging parsing logic.
*   `--codegen`: Run through code generation but stop before linking. Useful for inspecting the IR.
*   `--emit-asm` (`-S`): Compile the code to assembly language (`.s` file) but do not assemble or link it.
*   `--keep-intermediates`: Keep the intermediate files generated during compilation (e.g., `.i` preprocessed file, `.s` assembly file). By default, these are cleaned up.
*   `--safe-malloc`: Link against a safe memory allocator runtime that detects buffer overflows and use-after-free errors.
*   `--help` (`-h`): Print usage information and the list of available options.
*   `--version` (`-V`): Print the compiler version.

One of the driver's key roles is to invoke the C preprocessor (via `gcc -E`) before passing the code to the lexer. This handles `#include`, `#define`, and other preprocessor directives. The driver also calls `gcc` at the end to assemble the generated assembly code and link it into an executable. This design simplifies the compiler by leveraging existing system tools for preprocessing and linking.
