# Model

The **Model** crate defines every shared data type that the rest of the compiler depends on. It has zero external dependencies and is the root of the crate dependency graph — every other crate in the workspace imports from `model`.

## What lives here

### `lib.rs` — AST and Token definitions

This single file contains the complete set of types that represent a parsed C program:

**`Token`** (~100 variants) — the output of the lexer, input of the parser. Covers:
- Literals: `Constant { value: i64, suffix: IntegerSuffix }`, `FloatLiteral(f64)`, `StringLiteral(String)`, `Identifier`
- `IntegerSuffix` enum: `None`, `U`, `L`, `UL`, `LL`, `ULL` — tracks the suffix on integer constants for correct type determination
- Punctuation: parens, braces, brackets, semicolons, commas, colons
- All C keywords: `int`, `void`, `return`, `if`, `for`, `while`, `switch`, `struct`, `union`, `enum`, `typedef`, `sizeof`, `static`, `extern`, `const`, `volatile`, etc.
- C99/C11 keywords: `_Bool`, `_Generic`, `_Alignof`, `_Static_assert`, `register`, `restrict`
- GCC internals: `__attribute__`, `__extension__`, `typeof`/`__typeof__`
- Operators: arithmetic, relational, logical, bitwise, assignment, compound assignment, increment/decrement, arrow, ellipsis

**`Type`** — represents C types in the AST:
- Scalar: `Int`, `UnsignedInt`, `Char`, `UnsignedChar`, `Short`, `UnsignedShort`, `Long`, `UnsignedLong`, `LongLong`, `UnsignedLongLong`, `Float`, `Double`, `Bool`, `Void`
- Compound: `Array(element_type, size)`, `Pointer(pointee)`, `Struct(name)`, `Union(name)`, `Typedef(name)`
- `FunctionPointer { return_type, param_types }`
- `TypeofExpr(expr)` — deferred to IR lowering for resolution

**`Expr`** (~23 variants) — every expression form the compiler handles:
- `Binary`, `Unary`, prefix/postfix increment/decrement
- `Variable`, `Constant`, `FloatConstant`, `StringLiteral`
- `Index` (array subscript), `Call` (direct and indirect), `Cast`
- `Member` / `PtrMember` (`.` and `->`)
- `SizeOf(Type)`, `SizeOfExpr`, `AlignOf(Type)`
- `Conditional` (ternary `?:`), `Comma` (comma operator)
- `CompoundLiteral`, `StmtExpr` (GNU statement expression), `InitList`
- `BuiltinOffsetof`, `Generic` (C11 `_Generic` selection)

**`Stmt`** — all statement forms: `Return`, `If`, `While`, `DoWhile`, `For`, `Switch`, `Case`, `Default`, `Break`, `Continue`, `Goto`, `Label`, `Declaration`, `MultiDecl`, `InlineAsm`, `Block`, `Expr`

**`Attribute`** — GCC `__attribute__` variants: `Packed`, `Aligned(N)`, `Section(name)`, `NoReturn`, `AlwaysInline`, `Weak`, `Unused`, `Constructor`, `Destructor`

**Supporting types**: `TypeQualifiers` (`const`/`volatile`/`restrict`), `BinaryOp` (20 variants including compound assignment), `UnaryOp`, `InitItem`/`Designator` for initializer lists, `AsmOperand` for inline assembly, `Program`/`Function`/`GlobalVar`/`FunctionPrototype`/`StructDef`/`UnionDef`/`EnumDef`/`Block`/`StructField`.

**`Program`** contains:
- `functions: Vec<Function>` — function definitions with bodies
- `globals: Vec<GlobalVar>` — global variable declarations (with `is_extern` and `is_static` flags)
- `structs`, `unions`, `enums` — type definitions
- `prototypes: Vec<FunctionPrototype>` — function declarations without bodies (return type, name, params, variadic flag)
- `forward_structs: Vec<String>` — forward-declared struct names (`struct foo;`)

**`Function`** includes `is_static: bool` — controls linkage visibility (`.globl` vs local). **`GlobalVar`** includes `is_extern: bool` and `is_static: bool`.

### `target.rs` — Platform abstraction

Defines `Platform` (Windows/Linux), `CallingConvention` (WindowsX64/SystemV), and `TargetConfig`. Auto-detects the host platform at compile time via `cfg!` macros. Used by the driver to select executable extensions and by codegen to select calling conventions, shadow space sizes, and callee-saved register sets.

## Design decisions

- All types derive `Debug`, `PartialEq`, and `Clone`. This enables test assertions (`assert_eq!`) and allows the parser and IR lowerer to freely clone AST subtrees.
- `Token` is a flat enum (no trait objects, no Box indirection) for efficient pattern matching in the parser.
- Recursive types (`Expr` contains `Box<Expr>`, `Type` contains `Box<Type>`) use heap allocation only where structurally necessary.
- The crate deliberately has no logic — no parsing, no validation, no codegen. It is a pure data definition layer.
