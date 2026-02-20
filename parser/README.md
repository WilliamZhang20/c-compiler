# Parser

The **Parser** crate transforms the token stream produced by the lexer into an Abstract Syntax Tree (AST). It implements a **recursive-descent** parser where each grammar production maps to a function. The public entry point is `parse_tokens(tokens) -> Result<Program, String>`, which returns a `Program` containing functions, global variables, struct/union/enum definitions.

## Architecture

The core `Parser` struct (in `parser.rs`) holds the token slice, a cursor position, and a set of known `typedef` names. Parsing logic is split across several files, each exposed to `Parser` via a trait (e.g. `ExpressionParser`, `StatementParser`). This keeps individual files focused while sharing the same parser state.

## Source Files

### `parser.rs`
Defines the `Parser` struct and low-level token navigation: `peek()`, `advance()`, `match_token()`, `expect()`, and `check()`. This file contains no grammar rules — those are distributed across the trait implementations below. The typedef set is pre-seeded with `__builtin_va_list`.

### `declarations.rs`
Top-level program parsing via the `DeclarationParser` trait. `parse_program()` loops over tokens, distinguishing between function definitions, global variable declarations, struct/union/enum definitions, typedefs, and extern declarations. It also gracefully skips unsupported constructs from system headers (extern inline functions, forward declarations) so that pre-processed source can be compiled. Helper methods include `parse_function()`, `parse_function_params()`, `parse_globals()`, and `parse_typedef()`.

### `expressions.rs`
Expression parsing via the `ExpressionParser` trait, using **precedence climbing**. Each precedence level has its own method, called in order from lowest to highest:

assignment → conditional (ternary) → logical or → logical and → bitwise or → xor → bitwise and → equality → relational → shift → additive → multiplicative → unary → postfix → primary

This handles all C expression forms: binary and compound-assignment operators, ternary `?:`, prefix/postfix increment/decrement, `sizeof`, type casts, function calls, array indexing, member access (`.` and `->`), and primary atoms (identifiers, constants, string literals, parenthesized sub-expressions).

### `statements.rs`
Statement parsing via the `StatementParser` trait. `parse_stmt()` dispatches on the current token to handle `return`, `break`, `continue`, `goto`, labeled statements, `if`/`else`, `while`, `do`/`while`, `for` (including C99 init-declarations), `switch`/`case`/`default`, inline assembly (`asm`), block scopes, local variable declarations, and expression statements. `parse_block()` handles `{ ... }` sequences.

### `types.rs`
Type parsing via the `TypeParser` trait. `parse_type_with_qualifiers()` handles storage-class specifiers (`static`, `extern`, `inline`), type qualifiers (`const`, `volatile`, `restrict`), GCC attributes, and the full set of base types including `char`, `short`, `int`, `long`, `long long`, `float`, `double`, `unsigned`/`signed` variants, `void`, `struct`/`union`/`enum` types, typedef names, and pointer/array/function-pointer declarators. Also contains `parse_struct_definition()`, `parse_union_definition()`, and `parse_enum_definition()` for aggregate type bodies.

### `attributes.rs`
GCC `__attribute__((...))` parsing via the `AttributeParser` trait. Recognizes `packed`, `aligned(N)`, `section("name")`, `noreturn`, `always_inline`, `format(...)`, `interrupt`, and `signal`. Unknown attributes are skipped. The parsed `Attribute` values are attached to functions and struct definitions in the AST.

### `utils.rs`
Lookahead and skip utilities via the `ParserUtils` trait. Contains heuristics for distinguishing function definitions from declarations (`is_function_definition()`), detecting inline/extern functions from headers, and skipping over constructs the parser does not yet fully support (forward declarations, extern declarations, top-level items). Also provides `check_is_type()` for disambiguating type names from identifiers in contexts like casts and declarations.
