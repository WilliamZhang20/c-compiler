# Intermediate Representation (IR)

The **IR** crate defines the compiler's intermediate representation and implements the lowering pass that translates the semantic AST into it. The IR is a **basic-block CFG in SSA form**: each function is a list of `BasicBlock`s containing typed `Instruction`s and ending with a `Terminator`. Variables use SSA naming (`VarId`), and cross-block value merges are expressed with `Phi` nodes. This representation is consumed by the optimizer and, after phi removal, by the code generator.

## Architecture

The lowering pipeline is: AST → `Lowerer` → `IRProgram` (with SSA via on-the-fly phi insertion) → `mem2reg` (promotes stack slots to registers) → optimizer → `remove_phis` (deconstructs phis into copies) → codegen.

## Source Files

### `lib.rs`
Module root. Re-exports the key IR types (`Instruction`, `Terminator`, `BasicBlock`, `Function`, `IRProgram`, `VarId`, `BlockId`, `Operand`) and the `Lowerer`, `mem2reg`, `remove_phis`, and `verify_ssa` entry points. Contains integration tests that lower small C programs and verify the resulting instruction shapes.

### `types.rs`
Defines every IR data structure:

- **`VarId` / `BlockId`** — newtypes wrapping `usize`, used as SSA variable and basic-block identifiers.
- **`Operand`** — `Constant(i64)`, `FloatConstant(f64)`, `Var(VarId)`, or `Global(String)`.
- **`Instruction`** — 16-variant enum covering `Binary`, `FloatBinary`, `Unary`, `FloatUnary`, `Copy`, `Cast`, `Phi`, `Alloca`, `Load`, `Store`, `GetElementPtr`, `Call`, `IndirectCall`, `InlineAsm`, and variadic intrinsics (`VaStart`, `VaEnd`, `VaCopy`, `VaArg`).
- **`Terminator`** — `Br`, `CondBr`, `Ret`, `Unreachable`.
- **`BasicBlock`** — holds its `id`, instruction list, terminator, and an `is_label_target` flag for goto destinations.
- **`Function`** / **`IRProgram`** — top-level containers. `Function` carries a `var_types: HashMap<VarId, Type>` map so that type annotations (e.g. float vs int) survive across optimization passes and reach codegen. `IRProgram` also stores global strings, global variables, and struct/union definitions.

### `lowerer.rs`
The `Lowerer` struct and its `lower_program()` / `lower_function()` methods. This is the main AST → IR translation engine. It maintains extensive state: SSA bookkeeping (variable definitions, incomplete phis, sealed blocks), symbol tables (locals, globals, structs, unions, enums, typedefs), control-flow context (loop break/continue targets, switch case lists, goto labels with forward-reference resolution), and type-size caches.

Key responsibilities include parameter spilling to `Alloca` slots (so addresses can be taken), type-size and struct-layout computation (with alignment and `__attribute__((packed))` support), and coordinating block creation and sealing for correct SSA construction.

### `expressions.rs`
### `expressions.rs`\nImplements `lower_expr()` and `lower_to_addr()` on `Lowerer`. `lower_expr()` dispatches on every AST expression variant: constants, variables (with array-to-pointer decay), binary/unary operations (with separate int and float paths), assignments and compound assignments, pointer arithmetic (with element-size scaling), string literals (registered as global data), function calls (direct and indirect, including `__builtin_va_*` intrinsics), `sizeof`, type casts (int↔float, pointer casts), and pre/post increment/decrement. Function call lowering re-reads the current block after evaluating all arguments to correctly handle arguments that create new basic blocks (e.g., ternary expressions).

`lower_to_addr()` computes the memory address of an l-value, handling variables (alloca or global), array/pointer indexing via `GetElementPtr`, dereferences, and struct/union member access with byte-offset calculation.

### `statements.rs`
Implements `lower_stmt()` and `lower_block()` on `Lowerer`. Handles all statement-level control flow translation:

- **Declarations** — creates `Alloca` slots, handles initializers including character-array string initialization.
- **If/else** — creates then/else/merge blocks with `CondBr`.
- **Loops** (`while`, `do-while`, `for`) — creates header/body/exit blocks with proper block sealing order to support back-edge phi construction; pushes loop context for `break`/`continue`.
- **Switch** — collects case/default blocks, builds a linear comparison chain in the head block, supports fallthrough.
- **Goto/Label** — creates target blocks and resolves forward references via a `pending_gotos` list.
- **Return** — emits `Ret` terminator with optional float↔int cast.
- **Inline assembly** — maps output/input operands to IR variables and emits `InlineAsm`.

Dead code after terminators is handled by setting `current_block` to `None`.

### `ssa.rs`
Implements on-the-fly SSA construction using the **Braun et al.** algorithm. Functions on `Lowerer`:

- **`write_variable` / `read_variable`** — record and look up the current SSA definition of a named variable within a block. When a definition is not found locally, `read_variable_recursive` walks predecessors to insert `Phi` nodes as needed.
- **`seal_block`** — marks a block as having all predecessors known, resolving any deferred incomplete phis.
- **`get_predecessors`** — computes (and caches) the predecessor list for a block by scanning all terminators.

Phi nodes are only inserted when a variable is live across multiple predecessors, avoiding unnecessary phis by construction.

### `mem2reg.rs`
The **mem2reg** optimization pass, which promotes scalar `Alloca`/`Load`/`Store` patterns to SSA registers. An alloca is promotable if it is a scalar type (integers, floats, or pointers) and is only used as the address operand of `Load` and `Store` instructions (its address never escapes). Address-taken analysis checks all instruction operands including `IndirectCall` function pointers.

The pass replaces `Load`s with the reaching definition and removes the corresponding `Alloca` and `Store` instructions. Cross-block value flow is resolved by inserting phi nodes, with trivial phi elimination (all operands identical). Single-predecessor incoming values are cached to avoid redundant recursion on long chains. Newly created phi vars are annotated with the alloca's type in `func.var_types` so codegen can distinguish float from integer variables.

When a phi is simplified away, a **comprehensive fixup pass** resolves all references to simplified phi vars across **every instruction type and terminator** — not just other phi nodes. The fixup resolves transitive simplification chains (e.g. `VarId(77)→76→75`) to their final target.

Uninitialized reads default to zero (int or float as appropriate). A separate `remove_phis()` function later deconstructs phi nodes into `Copy` instructions placed at the end of predecessor blocks, preparing the IR for register allocation.

The module also exports a `verify_ssa()` function that checks every used `VarId` is defined by a parameter or instruction dest. This runs as a `debug_assert!` after every mem2reg pass, catching undefined-variable bugs before they become runtime segfaults in codegen.
