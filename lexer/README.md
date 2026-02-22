# Lexer

The **Lexer** crate is the first stage of the compilation pipeline. It converts raw C source text (already preprocessed by `gcc -E`) into a flat stream of `Token` values that the parser consumes. 

**Public API**: `lexer::lex(input: &str) -> Result<Vec<Token>, String>`

## How it works

The lexer is a byte-oriented **state machine** (`StateMachineLexer`) that processes the input in a single forward pass. At each position it inspects the current byte to decide which sub-lexer to invoke — string, character, number, identifier/keyword, or operator. Whitespace, line comments (`//`), block comments (`/* */`), and residual preprocessor directives (`#...`) are consumed and discarded.

The lexer does not store line/column information. Errors are returned as strings describing what went wrong and roughly where.

## Source files

### `lib.rs`
Module root. Exposes `lex()`, which constructs a `StateMachineLexer` and calls `tokenize()`. Contains unit tests for basic tokenization (identifiers, keywords, operators, comments).

### `state_machine.rs`
The core lexer. `StateMachineLexer` holds a `&[u8]` input slice, a cursor position, and a `line_start` flag (for detecting preprocessor lines). Methods:

| Method | What it handles |
|---|---|
| `lex_next_token` | Top-level dispatch: inspects current byte and routes to the appropriate sub-lexer |
| `lex_string` | Double-quoted string literals with full escape support |
| `lex_char` | Character literals including **multi-character constants** (e.g. `'ABCD'` → packed big-endian `i64`) |
| `lex_number` | Decimal integers and floating-point literals (`3.14`, `1e-5`, `.5f`). Detects `0x`/`0b`/`0` prefixes and dispatches to hex/binary/octal sub-lexers. Consumes **integer suffixes**: `U`, `L`, `UL`, `LL`, `ULL` (case-insensitive) via `parse_integer_suffix()` |
| `lex_hex_number` | Hexadecimal integers (`0xFF`). Also consumes integer suffixes |
| `lex_octal_number` | Octal integers (`0777`, `0644`). Digits 0-7 only |
| `lex_binary_number` | Binary integers (`0b1010`, `0B1111`). GCC extension |
| `lex_identifier` | `[a-zA-Z_][a-zA-Z0-9_]*` identifiers, then delegates to `keywords::keyword_or_identifier` |
| `lex_operator_or_punctuation` | Single/two/three-character operators: `==`, `!=`, `<=`, `>=`, `&&`, `||`, `->`, `++`, `--`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `...` |
| `skip_line_comment` / `skip_block_comment` | Comment consumption |

**Escape sequences** supported in strings and characters: `\n`, `\t`, `\r`, `\\`, `\'`, `\"`, `\0`, `\a`, `\b`, `\f`, `\v`, hex (`\x1F`), and octal (`\077`).

**Floats starting with `.`**: The lexer recognizes `.123` as `FloatLiteral(0.123)` by checking for a digit after the dot before treating `.` as an operator.

### `keywords.rs`
Maps identifier strings to keyword tokens via a single `keyword_or_identifier(s: &str) -> Token` function. ~85 keyword mappings including:

- Standard C: `int`, `void`, `return`, `if`, `else`, `while`, `for`, `do`, `break`, `continue`, `goto`, `switch`, `case`, `default`, `struct`, `union`, `enum`, `typedef`, `sizeof`, `static`, `extern`, `const`, `volatile`, `inline`
- C99/C11: `_Bool`, `_Generic`, `_Alignof`, `_Static_assert`, `restrict`, `_Noreturn`
- GCC extensions: `__attribute__`, `__extension__`, `__asm__`, `__volatile__`, `__inline__`, `__restrict__`, `__typeof__`, `__alignof__`, `typeof`, `asm`, `__auto_type`, `__label__`
- Calling conventions (mapped to `Extension`): `__cdecl`, `__stdcall`, `__fastcall`, `__thiscall`, `__vectorcall`
- Size-related: `short`, `long`, `signed`, `unsigned`, `char`, `float`, `double`
- Qualifiers: `register`, `noreturn`

Anything not in the map becomes `Token::Identifier { value }`.

### `literals.rs`
Pure functions for parsing literal values from already-delimited text:

- `parse_char_literal(s: &str) -> i64` — converts a char body (`n`, `\\`, `x1F`, `077`) to its integer value
- `parse_int_constant(s: &str) -> i64` — decimal, `0x`-prefixed hex, `0`-prefixed octal, or `0b`-prefixed binary
- `parse_float_literal(s: &str) -> f64` — strips optional `f`/`F` suffix

### `repro_bug.rs`
Test-only module (`#[cfg(test)]`) for reproducing and regression-testing specific lexer bugs (float-starting-with-dot, compound assignment, escape sequences).
