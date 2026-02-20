# Semantic Analyzer

The **Semantic Analyzer** crate validates the parsed AST before it is lowered to IR. It performs name resolution, qualifier enforcement, and control-flow validity checks. The public entry point is `SemanticAnalyzer::analyze(program) -> Result<(), String>`.

## Architecture

The analyzer is implemented as a single-file, single-pass checker in `lib.rs`. It walks every function body, recursively visiting statements and expressions. Errors are returned immediately (fail-fast, no multi-error accumulation).

## Source File

### `lib.rs`

The `SemanticAnalyzer` struct maintains:

- **Scope stack** — a `Vec<HashMap<String, Type>>` implementing lexical scoping. `enter_scope()` pushes a new frame; `exit_scope()` pops it. `lookup_symbol()` searches from innermost to outermost, providing standard shadowing semantics.
- **Global scope** — a separate `HashMap` for global variables and function signatures, cloned as the base of each function's scope stack.
- **Qualifier maps** — `const_vars` and `volatile_vars` track per-variable qualifier state for assignment checks.
- **Type definitions** — `structs`, `unions`, and `enum_constants` are registered globally at the start of analysis, before any function body is visited.
- **Control-flow depth** — `loop_depth` and `in_switch` track nesting to validate `break`, `continue`, `case`, and `default` placement.

**Checks performed:**

- Use of undeclared variables (with a carve-out for direct function calls, allowing implicit declarations).
- Assignment or increment/decrement of `const`-qualified variables.
- `restrict` qualifier applied to non-pointer types.
- `break` outside a loop or switch; `continue` outside a loop.
- `case`/`default` outside a switch.
- Duplicate function definitions and duplicate enum constant names.
- Inline assembly operand validation (outputs and inputs are well-formed expressions).

**Scope handling per function:** The scope stack is reset to a clone of the global scope, then a fresh local scope is pushed for parameters. Each `Block` statement introduces and removes a scope. Global variable redeclarations are silently allowed to support `extern` forward declarations.

The analyzer does not perform type-compatibility checking on assignments or binary operations — it is primarily a name-resolution and qualifier-enforcement pass. Goto/label validation is deferred to the IR lowering stage.
