# Semantic Analyzer

The **Semantic Analyzer** validates the parsed AST before it is lowered to IR. It catches programmer errors that are syntactically valid but semantically wrong — using undeclared variables, modifying `const` values, putting `break` outside a loop, etc.

**Public API**: `SemanticAnalyzer::new()` then `analyzer.analyze(&program) -> Result<(), String>`

Errors are returned immediately (fail-fast — no multi-error accumulation).

## What it checks

| Check | Example |
|---|---|
| Undeclared variable use | `x = 5;` when `x` was never declared |
| `const` assignment | `const int x = 1; x = 2;` |
| `const` increment/decrement | `const int x = 1; x++;` |
| `restrict` on non-pointer | `restrict int x;` |
| `break` outside loop/switch | `break;` at function scope |
| `continue` outside loop | `continue;` inside a `switch` but not a loop |
| `case`/`default` outside switch | `case 1:` at function scope |
| Duplicate function definitions | Two functions with the same name and body |
| Duplicate enum constants | `enum { A, A };` |
| Inline asm operand validation | Malformed output/input operands |

## What it does NOT check

- **Type compatibility** on assignments or binary operations — the analyzer is primarily name-resolution and qualifier enforcement. Type mismatches (e.g. assigning a `float` to an `int*`) are not flagged.
- **Goto/label validation** — deferred to the IR lowering stage, which resolves forward references.
- **Function call arity/type matching** — not checked (allows implicit declarations for direct calls).

## How it works

### State
The `SemanticAnalyzer` struct maintains:

- **Scope stack** (`Vec<HashMap<String, Type>>`) — lexical scoping with `enter_scope()`/`exit_scope()`. `lookup_symbol()` searches innermost → outermost, providing standard variable shadowing.
- **Global scope** (`HashMap<String, Type>`) — global variables and function signatures. Cloned as the base of each function's scope stack.
- **Qualifier maps** — `const_vars` and `volatile_vars` track per-variable qualifier state.
- **Type definitions** — `structs`, `unions`, `enum_constants` registered globally before any function body is visited.
- **Control-flow counters** — `loop_depth` and `in_switch` for validating `break`/`continue`/`case`/`default` placement.

### Per-function analysis
1. Reset scope stack to a clone of global scope
2. Push a fresh local scope for parameters
3. Recursively visit all statements and expressions
4. Each `Block` statement pushes/pops a scope
5. Global variable redeclarations are silently allowed (supports `extern` patterns)

### Expression analysis
The `analyze_expr()` method recursively validates every sub-expression. For `Binary` expressions with `Assign` or compound-assignment operators, it checks whether the left-hand side is `const`. For `Generic` and `AlignOf` expressions, it validates the controlling expression and type arguments respectively. Member access, function calls, casts, and all other expression forms are traversed to ensure all referenced variables are in scope.

## Source file

Everything lives in a single `lib.rs` — the analyzer is ~350 lines and doesn't warrant splitting into multiple files.
