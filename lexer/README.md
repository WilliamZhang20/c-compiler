# Lexer

The **Lexer** crate is the first stage of the compilation pipeline. It converts raw C source text into a flat stream of `Token` values (defined in the `model` crate) that the parser consumes. The public entry point is `lex(input) -> Result<Vec<Token>, String>`.

## Architecture

The lexer is implemented as a byte-oriented **state machine** (`StateMachineLexer`). It processes the input in a single forward pass, classifying each byte to decide which sub-lexer to invoke (string, character, number, identifier/keyword, or operator). Whitespace, line comments (`//`), block comments (`/* */`), and preprocessor directives (`#...`) are consumed and discarded.

## Source Files

### `lib.rs`
Module root and public API. Exposes the `lex()` function, which constructs a `StateMachineLexer` and calls `tokenize()`. Also contains integration tests that verify round-trip lexing of identifiers, keywords, operators, and comments.

### `state_machine.rs`
Core lexer implementation. `StateMachineLexer` holds a byte-slice reference, a cursor position, and a line-start flag (used to detect preprocessor directives). Key methods:

- **`lex_next_token`** — top-level dispatch based on the current byte: routes to comment skipping, string/char lexing, number lexing, identifier lexing, or operator lexing.
- **`lex_string` / `lex_char`** — handle quoted literals including the full set of C escape sequences (octal `\077`, hex `\x1F`, and standard escapes like `\n`, `\t`, `\0`).
- **`lex_number` / `lex_hex_number`** — parse decimal integers, hexadecimal integers (`0x` prefix), and floating-point literals (with optional exponent and `f`/`F` suffix).
- **`lex_identifier`** — consumes `[a-zA-Z_][a-zA-Z0-9_]*` and delegates to `keywords::keyword_or_identifier` to classify the result.
- **`lex_operator_or_punctuation`** — recognizes single-char, two-char (`==`, `&&`, `->`, `++`, `+=`, …), and three-char (`...`, `<<=`, `>>=`) tokens.

### `keywords.rs`
A single `keyword_or_identifier(value) -> Token` function that maps identifier strings to keyword tokens. Covers all supported C keywords (`int`, `void`, `return`, `if`, `for`, `struct`, `enum`, `typedef`, `sizeof`, …) as well as GCC extensions and alternative spellings (`__asm__`, `__volatile__`, `__inline__`, `__attribute__`, `__restrict__`, calling-convention specifiers, etc.). Unrecognized identifiers are returned as `Token::Identifier`.

### `literals.rs`
Standalone helpers for parsing literal values out of already-extracted text:

- **`parse_char_literal`** — converts a character-literal body (e.g. `n`, `\\`, `x1F`, `077`) to its `i64` code-point value.
- **`parse_int_constant`** — parses decimal or `0x`-prefixed hexadecimal integer strings.
- **`parse_float_literal`** — parses floating-point strings, stripping an optional `f`/`F` suffix.

These are called by `state_machine.rs` after the raw text has been delimited.

### `repro_bug.rs`
Test-only module for reproducing and debugging specific lexer issues.
