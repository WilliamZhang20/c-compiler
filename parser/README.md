# Parser

The **Parser** crate transforms the token stream from the lexer into an Abstract Syntax Tree (AST). It implements a **recursive-descent** parser with **precedence climbing** for expressions, where each grammar production maps to a method.

**Public API**: `parser::parse_tokens(tokens: &[Token]) -> Result<Program, String>`

Returns a `Program` containing functions, global variables, struct/union/enum definitions, and typedefs.

## How it works

The `Parser` struct holds the token slice, a cursor position, and a set of known `typedef` names (pre-seeded with `__builtin_va_list`). Parsing logic is split across several files, each exposed to `Parser` via a trait — this keeps files focused while sharing the same parser state.

The parser handles preprocessed system headers by gracefully skipping constructs it doesn't fully support (extern inline functions, forward-only declarations). This means you can compile `#include <stdio.h>` code without the parser choking on glibc internals.

## Source files

### `parser.rs`
Core `Parser` struct and token navigation primitives: `peek()`, `advance()`, `match_token()`, `expect()`, `check()`. No grammar rules live here — those are in the trait implementations below.

### `declarations.rs` — `DeclarationParser` trait
Top-level program parsing. `parse_program()` loops over tokens and dispatches to:
- `parse_function()` — function definitions with parameters, body, attributes
- `parse_globals()` — global variable declarations with optional initializers
- `parse_typedef()` — type alias registration
- Struct/union/enum definitions at file scope
- Attribute parsing and propagation to the following declaration (handles `__attribute__((constructor))` before a function)

Also handles deduplication of extern forward declarations and skipping of unsupported header constructs.

### `expressions.rs` — `ExpressionParser` trait
Expression parsing via precedence climbing, from lowest to highest precedence:

```
assignment → conditional (ternary) → logical or → logical and
→ bitwise or → xor → bitwise and → equality → relational
→ shift → additive → multiplicative → unary → postfix → primary
```

Handles all expression forms:
- Binary operators (arithmetic, relational, logical, bitwise, compound assignment)
- Ternary `?:` including GNU extension for omitted middle operand
- Prefix/postfix `++`/`--`, unary `+`/`-`/`!`/`~`/`*`/`&`
- `sizeof(type)`, `sizeof expr`, `_Alignof(type)`
- Type casts: `(int)x`
- Function calls (direct and indirect via function pointers)
- Array indexing: `a[i]`
- Member access: `s.field`, `p->field`
- Primary atoms: identifiers, integer/float constants, string literals, parenthesized sub-expressions
- C11 `_Generic(expr, int: a, float: b, default: c)`
- GNU statement expressions: `({ stmts; expr; })`
- Compound literals: `(type){init_list}`
- Comma expressions: `(a, b, c)`
- GCC builtins: `__builtin_offsetof`, `__builtin_expect`, `__builtin_types_compatible_p`, `__builtin_choose_expr`, `__builtin_unreachable`, `__builtin_trap`, `__builtin_clz/ctz/popcount/abs`

### `statements.rs` — `StatementParser` trait
Statement parsing. `parse_stmt()` dispatches on the leading token:
- `return`, `break`, `continue`, `goto`, labels
- `if`/`else`, `while`, `do`/`while`, `for` (including C99 init-declarations)
- `switch`/`case`/`default` with fallthrough
- Block scopes `{ ... }`
- Local variable declarations (single and multi-variable)
- Inline assembly (`asm`/`__asm__`) with output/input operands and clobbers
- Expression statements
- `_Static_assert(expr, "message")`

**Constant expressions in array sizes**: `parse_array_size()` evaluates simple constant expressions at parse time using `const_eval_expr()`, `const_sizeof()`, and `const_alignof()` helpers. This supports declarations like `int buf[sizeof(int) * 2 + 1]`.

### `types.rs` — `TypeParser` trait
Type parsing. `parse_type_with_qualifiers()` handles:
- Storage-class specifiers: `static`, `extern`, `inline`
- Type qualifiers: `const`, `volatile`, `restrict`, `register`
- Base types: `char`, `short`, `int`, `long`, `long long`, `float`, `double`, `void`, `_Bool`
- `signed`/`unsigned` variants with proper multi-keyword parsing (`unsigned long long`)
- `struct`/`union`/`enum` type references and inline definitions
- Typedef name resolution (checks the typedef set to disambiguate from identifiers)
- Pointer declarators with qualifier chains
- Array declarators with constant-expression sizes
- Function pointer declarators: `int (*fp)(int, int)`
- `typeof(expr)` / `__typeof__(expr)`
- GCC attributes attached to types

### `attributes.rs` — `AttributeParser` trait
Parses `__attribute__((...))` syntax. Recognized attributes:
- `packed`, `aligned(N)`, `section("name")`
- `noreturn`, `always_inline`
- `weak`, `unused`
- `constructor`, `destructor`
- `format(...)`, `interrupt`, `signal`

Unknown attributes are skipped without error. Parsed `Attribute` values are attached to functions, globals, and struct definitions in the AST.

### `utils.rs` — `ParserUtils` trait
Lookahead and skip utilities:
- `is_function_definition()` — heuristic lookahead to distinguish function definitions from declarations
- `check_is_type()` — disambiguates type names from identifiers (checks keywords + typedef set)
- Header-construct detection and skipping for extern/inline/forward declarations
