# Driver

The **Driver** crate is the entry point of the compiler CLI. It coordinates the entire compilation pipeline, orchestrating the execution of the lexer, parser, semantic analyzer, optimizer, and code generator. It also interfaces with external tools like `gcc` for preprocessing and linking.

This component parses command-line arguments provided by the user and determines which actions to perform. It manages input and output files, handles intermediate files (like `.i` preprocessed source or `.s` assembly), and drives the flow of data between the various compiler stages.

## Command Line Arguments Example as Run from Home Directory

- Generate assembly only (creates hello_world.s)
```
cargo run -- hello_world.c -S
```

- Compile to executable (creates hello_world on Linux, hello_world.exe on Windows)
```
cargo run -- hello_world.c
```

- Compile with custom output name
```
cargo run -- hello_world.c -o my_program
```

- See tokens only
```
cargo run -- hello_world.c --lex
```

- See AST only  
```
cargo run -- hello_world.c --parse
```

- Run through codegen without assembling/linking
```
cargo run -- hello_world.c --codegen
```

- Keep intermediate files (.i preprocessed, .s assembly)
```
cargo run -- hello_world.c --keep-intermediates
```

- Enable debug logging
```
cargo run -- hello_world.c --debug
```

One of the driver's key roles is to invoke the C preprocessor (via `gcc -E`) before passing the code to the lexer. This handles `#include`, `#define`, and other preprocessor directives. The driver also calls `gcc` at the end to assemble the generated assembly code and link it into an executable. This design simplifies the compiler by leveraging existing system tools for preprocessing and linking.
