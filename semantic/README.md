# Semantic Analyzer

The **Semantic Analyzer** validates the parsed AST before it is lowered to IR. It catches programmer errors that are syntactically valid but semantically wrong — using undeclared variables, type mismatches on calls, modifying `const` values, putting `break` outside a loop, etc.

**Public API**: `SemanticAnalyzer::new()` then `analyzer.analyze(&program) -> Result<(), String>`

Errors are returned immediately (fail-fast — no multi-error accumulation).

## What it checks

| Check | Example |
|---|---|
| Undeclared variable use | `x = 5;` when `x` was never declared |
| `const` assignment | `const int x = 1; x = 2;` |
| `const` through pointers | `const int *p; *p = 5;` |
| `const` increment/decrement | `const int x = 1; x++;` |
| `restrict` on non-pointer | `restrict int x;` |
| `break` outside loop/switch | `break;` at function scope |
| `continue` outside loop | `continue;` inside a `switch` but not a loop |
| `case`/`default` outside switch | `case 1:` at function scope |
| Duplicate `case` values | `case 1:` and `case 1:` in the same switch |
| Duplicate function definitions | Two functions with the same name and body |
| Duplicate enum constants | `enum { A, A };` |
| Inline asm operand validation | Malformed output/input operands |
| **Integer promotions** | `char`/`short`/`_Bool` operands promoted per C rules |
| **Usual arithmetic conversions** | Mixed signed/unsigned binary ops |
| **Assignment compatibility** | RHS type checked against LHS (with decay) |
| **Return type checking** | Return expression checked against function return type |
| **Function call arity/types** | Arguments checked against `Program.prototypes` / definitions |
| **Lvalue validation** | Assignment targets must be modifiable lvalues |
| **Pointer subtraction** | `ptr - ptr` requires compatible pointee types |
| **Bitfield width** | Width must not exceed storage type |
| **`typedef` resolution** | Via shared `model::TypeEnv` |
| **`typeof(expr)`** | Resolved in expression context |

## What it does NOT check (yet)

- **Initializer shape checking** — designated/range init lists are accepted loosely
- **Implicit function declarations** — calls to unknown functions are not flagged (no `-Wimplicit-function-declaration`)
- **Incomplete type use** — e.g. `sizeof(struct Foo)` before definition
- **Storage class conflicts** — e.g. `static extern int x;`
- **Goto/label resolution** — forward references resolved at IR lowering

## How it works

### State
The `SemanticAnalyzer` struct maintains:

- **`TypeEnv`** (`model::typing`) — typedef map, struct/union/enum registration, prototype signatures, promotion and conversion rules
- **Scope stack** (`Vec<HashMap<String, Type>>`) — lexical scoping with `enter_scope()`/`exit_scope()`
- **Global scope** — globals and function signatures; cloned as the base of each function's scope stack
- **Qualifier maps** — `const_vars` and `volatile_vars` track per-variable qualifier state
- **Control-flow counters** — `loop_depth` and `in_switch` for validating `break`/`continue`/`case`/`default` placement

### Per-function analysis
1. Register function signature in `TypeEnv`
2. Reset scope stack to a clone of global scope
3. Push a fresh local scope for parameters
4. Recursively visit all statements and expressions
5. Each `Block` statement pushes/pops a scope

### Expression analysis
`analyze_expr()` returns the expression's `Type` (after promotions where applicable). Assignments, calls, returns, and binary operations use `TypeEnv` compatibility helpers. All sub-expressions are traversed for scope and qualifier checks.

## Source files

- `semantic/src/lib.rs` — analyzer implementation
- `model/src/typing.rs` — shared `TypeEnv`, `FunctionSig`, promotion/conversion/check helpers
