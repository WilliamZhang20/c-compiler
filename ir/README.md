# Intermediate Representation (IR)

The **IR** crate defines the compiler's intermediate representation and implements the lowering pass from AST to IR. The IR is a **basic-block CFG in SSA form**: each function is a list of `BasicBlock`s containing typed `Instruction`s, ending with a `Terminator`. Variables use SSA naming (`VarId`), and cross-block value merges are expressed with `Phi` nodes.

**Public API**:
- `Lowerer::new()` then `lowerer.lower_program(&program) -> Result<IRProgram, String>`
- `mem2reg(func)` — promotes stack allocations to SSA registers
- `remove_phis(func)` — deconstructs phi nodes into copies for codegen
- `verify_ssa(func)` — debug assertion that all used vars are defined

## Pipeline position

```
AST → Lowerer → IRProgram (with SSA) → mem2reg → optimizer → remove_phis → codegen
```

## IR data model (`types.rs`)

| Type | Description |
|---|---|
| `VarId(usize)` | SSA variable identifier |
| `BlockId(usize)` | Basic block identifier |
| `Operand` | `Constant(i64)`, `FloatConstant(f64)`, `Var(VarId)`, `Global(String)` |
| `Instruction` | 16 variants: `Binary`, `FloatBinary`, `Unary`, `FloatUnary`, `Copy`, `Cast`, `Phi`, `Alloca`, `Load`, `Store`, `GetElementPtr`, `Call`, `IndirectCall`, `InlineAsm`, `VaStart/End/Copy/Arg` |
| `Terminator` | `Br(block)`, `CondBr(cond, then, else)`, `Ret(operand)`, `Unreachable` |
| `BasicBlock` | instructions + terminator + `is_label_target` flag |
| `Function` | blocks + `var_types: HashMap<VarId, Type>` (survives through optimizer to codegen) + `is_static: bool` for internal linkage |
| `IRProgram` | functions + global strings + global variables + struct/union definitions |

## Source files

### `lowerer.rs`
The main AST → IR translation engine. The `Lowerer` struct maintains:
- **SSA bookkeeping**: current definitions per variable/block, incomplete phis, sealed blocks
- **Symbol tables**: locals, globals, structs, unions, enums, typedefs
- **Control-flow context**: loop break/continue targets, switch case lists, goto labels with forward-reference resolution
- **Type-size caches**: memoized struct sizes and member offsets

Parameters are spilled to `Alloca` slots so their addresses can be taken. Delegates to `expressions.rs` and `statements.rs` for the actual lowering logic.

### `expressions.rs`
Implements `lower_expr()`. Dispatches on every AST expression variant:
- Constants, variables (with array-to-pointer decay)
- Binary/unary operations with separate int and float instruction paths
- Assignments and compound assignments
- Pointer arithmetic with element-size scaling
- String literals (registered as global data)
- Function calls (direct and indirect, including `__builtin_va_*` intrinsics)
- `sizeof`, `_Alignof` — resolved to integer constants
- Type casts (int↔float, pointer casts, bool truncation)
- Pre/post increment/decrement
- `_Generic` selection — resolved at IR time using `types_compatible()` and `get_expr_type()` to match against the controlling expression's type
- GCC builtins: `__builtin_clz/ctz/popcount` (compile-time eval for constants), `__builtin_abs` (inline codegen), `__builtin_unreachable/trap` (emit `Unreachable` terminator)

After evaluating function call arguments, `lower_expr` re-reads `self.current_block` because argument evaluation may have created new blocks (e.g. from ternary expressions inside arguments).

### `lvalue.rs`
Implements `lower_to_addr()`. Computes the memory address of an l-value:
- Variables → alloca address or global symbol
- Array/pointer indexing → `GetElementPtr`
- Dereferences → the pointer value itself
- Struct/union member access → byte-offset from base via `GetElementPtr`

### `statements.rs`
Implements `lower_stmt()` and `lower_block()`:
- **Declarations** → `Alloca` + optional initializer stores (delegates init lists to `init_list.rs`)
- **If/else** → then/else/merge blocks with `CondBr`
- **Loops** → header/body/exit blocks with proper sealing order for back-edge phi construction; pushes loop context for break/continue
- **Switch** → case/default blocks, linear comparison chain in head block, fallthrough support
- **Goto/Label** → creates target blocks, resolves forward refs via `pending_gotos`
- **Return** → `Ret` terminator with optional float↔int cast
- **Inline assembly** → maps operands to IR variables, emits `InlineAsm`

Dead code after terminators is handled by setting `current_block` to `None`.

### `init_list.rs`
Handles `lower_init_list_to_stores()` and `lower_struct_init_list()` for both positional and designated initializers. Supports nested initializer lists for arrays of structs. For unions, initializes only the first field per C standard.

### `type_utils.rs`
Type size and alignment helpers: `get_type_size()`, `get_alignment()`, `is_float_type()`, `get_member_offset()`. Handles struct padding, `__attribute__((packed))`, and typedef resolution.

### `ssa.rs`
On-the-fly SSA construction using the **Braun et al.** algorithm:
- `write_variable(name, block, var)` / `read_variable(name, block) -> Operand` — record and look up SSA definitions
- `read_variable_recursive` — walks predecessors to insert `Phi` nodes when definitions cross block boundaries
- `seal_block(block)` — marks a block as having all predecessors known, resolving deferred incomplete phis
- `get_predecessors(block)` — computes and caches predecessor lists by scanning terminators

Phi nodes are only inserted when actually needed (a variable is live across multiple predecessors).

### `mem2reg.rs`
Promotes scalar `Alloca`/`Load`/`Store` patterns to SSA registers. An alloca is promotable if:
1. It's a scalar type (int, float, pointer — not arrays/structs)
2. Its address never escapes (only used as the direct address operand of `Load`/`Store`)

The pass replaces `Load`s with reaching definitions, removes dead `Alloca`/`Store` instructions, and inserts phi nodes for cross-block value flow. Trivial phis (all operands identical) are eliminated. A comprehensive fixup pass resolves all references to simplified phi vars across every instruction type — not just other phi nodes. Transitive simplification chains are resolved. Uninitialized reads default to zero.

New phi variables are annotated with the alloca's `Type` in `func.var_types` so codegen can distinguish float from integer register classes.

### `ssa_utils.rs`
Standalone utilities:
- `verify_ssa(func)` — validates every used `VarId` is defined by a parameter or instruction. Runs as `debug_assert!` after mem2reg.
- `remove_phis(func)` — deconstructs phi nodes into `Copy` instructions at predecessor block ends, preparing IR for register allocation.
