# Linux Kernel Compilation — Gap Analysis

A comprehensive audit of the C compiler codebase against the requirements for compiling the Linux kernel (v6.x, x86-64). The kernel is written in GNU C (C11 with extensive GCC extensions) and requires a compiler that supports freestanding mode, inline assembly with full constraints, atomics, bitfields, and dozens of GCC `__attribute__` and `__builtin_*` extensions.

This document catalogs every identified gap organized by compiler stage.

---

## Project status (refreshed 2026-06-02)

**Integration tests**: 177 C programs in `testing/`; run `cargo test`.

**Semantic analysis** (2026-06-02): `model::TypeEnv` drives type inference, integer promotions, assignment/call checking, typedef/`typeof` resolution, duplicate `case` detection, and `const` through pointers.

**Driver flags added**: `-fPIC`/`-fpic`, `-fPIE`/`-fpie`, `-fprofile-generate`, `-fprofile-use=FILE`.

**PGO**: Built-in text profile format (`func:block count`); no external profiling libraries required. Instrumentation emits `__profc_*` counters; `-fprofile-use` guides block layout after the main optimizer pipeline.

**Benchmarks** (`benchmarks/run_benchmarks.sh`): compares this compiler (release build) against **GCC -O0**, **GCC -O2**, and **GCC -O3**. Latest Linux numbers: `benchmarks/results_linux.md`.

**Optimizer pipeline** (14 passes, see `optimizer/README.md`): mem2reg → algebraic → strength → copy prop → load forwarding → CSE → fold/DCE → **loop interchange** → **LICM** → **prefetch** → **auto-vectorization** (packed/strided/indexed gather-scatter, aggressive polyhedral nest gate, `mem_dependence`) → phi removal → CFG simplify → **block layout** (`__builtin_expect` hints).

---

## Table of Contents

1. [Preprocessor & Driver](#1-preprocessor--driver)
2. [Lexer & Tokens](#2-lexer--tokens)
3. [Type System (AST Model)](#3-type-system-ast-model)
4. [Parser & Declarations](#4-parser--declarations)
5. [GCC Attributes](#5-gcc-attributes)
6. [GCC Builtins & Intrinsics](#6-gcc-builtins--intrinsics)
7. [Semantic Analysis](#7-semantic-analysis)
8. [IR Representation](#8-ir-representation)
9. [Optimizer](#9-optimizer)
10. [Code Generation & ABI](#10-code-generation--abi)
11. [Assembly Output & ELF](#11-assembly-output--elf)
12. [Inline Assembly](#12-inline-assembly)
13. [Linker & Object File Support](#13-linker--object-file-support)
14. [Summary & Priority Tiers](#14-summary--priority-tiers)

---

## 1. Preprocessor & Driver

The compiler delegates preprocessing to `gcc -E -P`. This covers `#include`, `#define`, `#ifdef`, etc., but the driver itself has significant gaps.

### Currently Supported
- `gcc -E -P` for preprocessing with **`-D` / `-U` / `-I` / `--include`** passthrough
- `gcc` for assembling + linking
- **`-c`** (compile to `.o` without linking)
- **`-nostdlib` / `--ffreestanding`** forwarded to linker/preprocessor
- Multiple source file compilation (each compiled to `.s` or `.o`, then linked)
- `-o`, `-S` (emit asm), `--keep-intermediates`, `--debug` flags

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **`-fno-builtin`** | **Critical** — kernel defines its own `memcpy`, `memset`, etc. | Not supported as driver flag |
| **`-include` (force-include file)** | **High** — kernel uses `-include include/linux/compiler_types.h` | Not supported |
| **`-fPIC` / `-fPIE`** | **Medium** — kernel modules need PIC | No PIC/PIE code generation |
| **`-mcmodel=kernel`** | **High** — kernel runs in upper 2GB of virtual address space | No memory model support |
| **`-march=` / `-mtune=`** | **Medium** — kernel sets minimum ISA level | No target architecture flags |
| **`-std=gnu11`** | **Low** — informational; behavior should match | No standard selection |
| **`-Wl,...` linker flag passthrough** | **High** — kernel passes linker scripts | Not supported |
| **`-shared`** | **Medium** — kernel modules are relocatable objects | Not supported |
| **`-g` (DWARF debug info)** | **Medium** — needed for `CONFIG_DEBUG_INFO` | No debug information generation |
| **`-Werror` / warning control** | **Low** — kernel compiles with `-Werror` | No warning infrastructure |
| **`-fno-strict-aliasing`** | **High** — kernel requires this | No strict aliasing analysis exists, so effectively already off |
| **`-fno-common`** | **Medium** — default in GCC 10+; kernel relies on it | All globals emitted as definitions (no `.comm`), so effectively already on |
| **`-mno-red-zone`** | **Critical** — kernel code cannot use the red zone | No flag; red zone usage unknown |
| **`-fno-stack-protector`** | **High** — kernel has its own stack protector | Not supported |
| **`-mno-80387` / `-mno-mmx` / `-mno-sse`** | **Critical** — kernel code must not use FPU/SSE | Float codegen uses SSE unconditionally |
| **`-fno-omit-frame-pointer`** | **Medium** — needed for reliable stack traces | Frame pointer behavior not configurable |

---

## 2. Lexer & Tokens

### Currently Supported
- All C11 keywords including `_Bool`, `_Generic`, `_Static_assert`, `_Alignof`, `_Alignas`, `_Noreturn`
- GCC extension keywords: `__attribute__`, `__extension__`, `__builtin_va_list`, `typeof`, `asm`/`__asm__`
- Decimal and hex integer literals, float literals, string and char literals
- All C operators including compound assignment, ternary, comma
- Standard escape sequences in strings/chars

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Octal integer literals (`0777`)** | ~~Critical~~ | ✅ Lexer (`lex_octal_number`); see Tier 0 |
| **Binary literals (`0b1010`)** | ~~Medium~~ | ✅ Lexer; see Tier 0 |
| **Hex float literals (`0x1.8p+1`)** | **Low** — rarely used in kernel | Not supported |
| **Integer suffix preservation (`42U`, `42UL`, `42ULL`, `42L`)** | **Critical** — all suffixes are discarded; everything becomes `i64` | Type of constant affects expression type in arithmetic |
| **Wide string literals (`L"..."`)** | **Low** — rarely used in kernel | Not supported |
| **Unicode string literals (`u8"..."`, `u"..."`, `U"..."`)** | **Low** — not used in kernel | Not supported |
| **`\u` / `\U` universal character names** | **Low** — not used in kernel | Not supported |
| **`_Atomic` keyword** | **High** — C11 atomics header uses this | Not in lexer keyword table |
| **`_Thread_local` keyword** | **Medium** — per-CPU variables in kernel | Not in lexer keyword table |
| **`_Complex` / `_Imaginary` keywords** | **Low** — not used in kernel | Not in lexer keyword table |
| **Multi-character constants (`'ABCD'`)** | **Medium** — used for magic numbers in some kernel code | Unknown if supported |
| **`\a` (alert) escape sequence** | **Low** | May be missing |

---

## 3. Type System (AST Model)

### Currently Supported Types
`Int`, `UnsignedInt`, `Char`, `UnsignedChar`, `Short`, `UnsignedShort`, `Long`, `UnsignedLong`, `LongLong`, `UnsignedLongLong`, `Void`, `Float`, `Double`, `Bool` (`_Bool`), `Array(Type, usize)`, `Pointer(Type)`, `Struct(String)`, `Union(String)`, `Typedef(String)`, `FunctionPointer`, `TypeofExpr(Expr)`

### Missing Types

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **`__int128` / `unsigned __int128`** | **High** — used in 128-bit arithmetic (e.g., `div_u64_rem`) | No AST type variant |
| **`long double` (80-bit x87)** | **Low** — not used in kernel (FPU disabled) | Parsed as `Double`; no distinct type |
| **`_Complex` types** | **Low** — not used in kernel | No AST type variant |
| **`_Atomic(T)` qualified types** | **High** — `<stdatomic.h>` pattern, some kernel C11 code | No AST type variant |
| **`_Thread_local` storage class** | **Medium** — kernel has its own per-CPU mechanism | No storage class tracking |
| **Variable-length arrays (VLA)** | **Medium** — kernel banned VLAs (since 4.20) but parser should still reject them gracefully | `Array` size is `usize` (fixed); no variable-length variant |
| **Enum as a type** | **High** — `enum foo x;` needs a `Type::Enum(String)` variant | Enum constants tracked as `i64` but no enum type for variables |
| **Qualified pointers** | **High** — `const int *` vs `int *const` vs `volatile int *` | `Pointer(Type)` has no qualifier field; qualifiers on pointee not propagated |
| **Incomplete array types (`int arr[]`)** | **High** — used in extern declarations, flexible array members | `Array` requires a fixed size |
| **Anonymous struct/union types** | **Medium** — `struct { int x; }` without a tag | `Struct(String)` requires a name |
| **Function types (not pointers)** | **Medium** — `typedef void (func_t)(int)` | Only `FunctionPointer` exists; no bare function type |
| **Typeof on types (`typeof(int *)`)** | **Medium** — kernel uses `typeof` on both exprs and types | Only `TypeofExpr` exists |
| **Bitfield type information** | **High** — bitfield width exists on `StructField` but doesn't affect layout | Bitfield packing/layout not computed |

---

## 4. Parser & Declarations

### Currently Supported
- Function definitions with parameters and local variables
- Struct/union definitions with fields (including bitfields)
- Enum definitions with explicit/implicit values
- Typedef declarations
- Global and local variable declarations with initializers
- Multi-declarators (`int a, b, c;`)
- Designator initializers (`.field = val`, `[idx] = val`)
- Compound literals (`(type){...}`)
- `_Generic` expressions
- Statement expressions (`({ ... })`)
- Inline assembly (`asm(...)` with operands)
- `_Static_assert` (parsed and validated)
- GCC `__attribute__` on functions, variables, struct fields
- Most C11 expression syntax

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Function prototypes / forward declarations** | **Critical** — kernel headers are full of declarations | Detected and silently skipped; not stored in AST. Means the compiler can't type-check calls to forward-declared functions |
| **`extern` variable declarations** | **Critical** — `extern int jiffies;` pattern | Detected and silently skipped; not in AST |
| **`static` functions/variables (internal linkage)** | **Critical** — kernel uses `static` extensively | Token consumed but linkage not tracked; all symbols emitted as `.globl` |
| **Forward struct declarations (`struct foo;`)** | **High** — opaque pointer pattern | Detected and silently skipped |
| **K&R-style function definitions** | **Low** — not used in modern kernel | Not supported |
| **Complex nested declarators** | **High** — `int (*(*fp)(int))(char)` pattern | Not supported; only simple function pointer declarators work |
| **Array of function pointers** | **High** — `void (*handlers[N])(int)` | May not parse correctly |
| **`typeof` in declarations** | **High** — `typeof(x) y;` is common in kernel macros | `TypeofExpr` exists but unclear if it works in all declaration contexts |
| **Variadic function flag** | **Medium** — `...` parsed but no is_variadic flag stored on function | Variadic calling convention may not be fully correct |
| **Nested designated initializers** | **Medium** — `.a.b = 5`, `.a[0].b = 3` | Only single-level `.field` and `[index]` designators |
| **Anonymous struct/union members** | **High** — `struct { struct { int x; }; }; s.x` | Partially supported; nested anonymous members may fail |
| **Flexible array members** | **High** — `struct { int n; char data[]; }` | Partially supported but size computation may be wrong |
| **Computed goto (`goto *ptr`)** | **High** — used in kernel interpreter dispatchers | Not parsed |
| **Label addresses (`&&label`)** | **High** — needed for computed goto | Not parsed |
| **`__label__` declarations** | **Low** — GCC local label extension | Not supported |
| **Attributes on types and statements** | **Medium** — `int __attribute__((aligned(4))) x;` | Only supported in specific positions |
| **`_Alignas` on struct fields** | **Medium** — alignment control on individual fields | May not be fully supported |
| **String literal concatenation** | **Medium** — `"hello " "world"` | Handled by GCC preprocessor (`-E`), so likely OK |
| **Designated initializer ranges** | **Medium** — `[0 ... 9] = val` (GCC extension) | Not supported |
| **Cast-to-union** | **Low** — GCC extension | Not supported |
| **`__auto_type`** | **Low** — GCC extension for type inference | Not supported |
| **Nested functions** | **Low** — GCC extension, not used in kernel | Not supported |

---

## 5. GCC Attributes

### Currently Supported
`packed`, `aligned(N)`, `section("name")`, `noreturn`, `always_inline`, `weak`, `unused`, `constructor`, `destructor`

### Missing (Kernel-Critical)

| Attribute | Kernel Relevance | Usage Example |
|-----------|-----------------|---------------|
| **`visibility("hidden"\|"default")`** | **Critical** — controls ELF symbol visibility | `__attribute__((visibility("hidden")))` |
| **`used`** | **Critical** — prevents linker from stripping symbol | `__attribute__((used))` on tracing/debugging data |
| **`alias("target")`** | **Critical** — symbol aliasing for weak/strong patterns | `__attribute__((alias("__real_func")))` |
| **`noinline`** | **Critical** — prevents inlining of specific functions | `noinline` annotation throughout kernel |
| **`cold` / `hot`** | **High** — code placement hints for error paths | `__cold` on unlikely-executed functions |
| **`cleanup(func)`** | **High** — automatic cleanup; `__free()`, guard patterns | `__attribute__((cleanup(free_fn)))` |
| **`deprecated` / `deprecated("msg")`** | **Medium** — deprecation warnings | API deprecation |
| **`format(printf, N, M)`** | **Medium** — format string checking | `printk` and friends |
| **`nonnull(N, ...)`** | **Low** — null pointer warnings | Pointer parameters |
| **`warn_unused_result`** | **Medium** — used on `must_check` functions | `__must_check` macro |
| **`may_alias`** | **High** — type-punning safety | Used in networking, crypto |
| **`mode(QI\|HI\|SI\|DI\|TI)`** | **Medium** — set storage size mode | Low-level type definitions |
| **`transparent_union`** | **Low** — union calling convention | Some syscall wrappers |
| **`vector_size(N)`** | **Low** — SIMD types (kernel avoids FPU) | SIMD crypto implementations |
| **`no_instrument_function`** | **Medium** — exclude from `-finstrument-functions` | Tracing infrastructure |
| **`pure` / `const`** | **Medium** — function has no side effects | Optimization hints |
| **`assume_aligned(N)`** | **Low** — pointer alignment guarantee | Allocator return values |
| **`fallthrough`** | **Medium** — `__attribute__((fallthrough))` in switch | Required with `-Wimplicit-fallthrough` |
| **`designated_init`** | **Low** — struct must use designated inits | Some kernel structures |
| **`error("msg")` / `warning("msg")`** | **High** — compile-time error/warning on call | `BUILD_BUK_ON_MSG` |
| **`externally_visible`** | **Low** — prevent IPO from removing | Symbols needed by modules |
| **`no_sanitize("...")`** | **Medium** — disable sanitizer for function | KASAN/KCSAN exclusions |
| **`copy(sym)`** | **Low** — copy attributes from another symbol | Some macro patterns |
| **`access(mode, ref, size)`** | **Low** — memory access annotation | `__read_only`, `__write_only` |

---

## 6. GCC Builtins & Intrinsics

### Currently Supported
| Builtin | Status |
|---------|--------|
| `__builtin_va_start` | IR instruction; codegen works |
| `__builtin_va_end` | IR instruction; codegen works |
| `__builtin_va_copy` | IR instruction; codegen works |
| `__builtin_va_arg` | IR + codegen (variadic access) |
| `__builtin_unreachable` | Lowered to `Unreachable` terminator |
| `__builtin_trap` | Treated as `Unreachable` |
| `__builtin_expect` | ✅ `Expr::Expect`; IR `BranchHint` for block layout |
| `__builtin_expect_with_probability` | ✅ Parsed as `Expect` (extra probability arg discarded) |
| `__builtin_constant_p` | Returns 0 unconditionally |
| `__builtin_offsetof` | Compile-time constant evaluation |
| `__builtin_clz` | Const-fold + runtime `bsr` (32-bit) |
| `__builtin_ctz` | Const-fold + runtime `bsf` (32-bit) |
| `__builtin_popcount` | Const-fold + runtime `popcnt` (32-bit) |
| `__builtin_abs` | Constant-folded; runtime uses `(x ^ (x>>31)) - (x>>31)` |

### Missing (Kernel-Critical)

| Builtin | Kernel Relevance | Notes |
|---------|-----------------|-------|
| **`__builtin_clzl/clzll/ctzl/ctzll/popcountl/popcountll`** | **High** — long/long long variants | ✅ Same inline codegen as 32-bit (64-bit `bsr`/`bsf`/`popcnt`) |
| **`__builtin_ffs/ffsl/ffsll`** | **High** — find first set bit | Not implemented |
| **`__builtin_bswap16/32/64`** | **Critical** — byte swapping for endianness | Not implemented |
| **`__builtin_memcpy`** | **Critical** — kernel's `memcpy` often routes through this | Not implemented |
| **`__builtin_memset`** | **Critical** — kernel's `memset` often routes through this | Not implemented |
| **`__builtin_memmove`** | **High** — overlapping memory copy | Not implemented |
| **`__builtin_memcmp`** | **Medium** — memory comparison | Not implemented |
| **`__builtin_strlen`** | **Medium** — compile-time string length | Not implemented |
| **`__builtin_strcmp`** | **Low** | Not implemented |
| **`__builtin_constant_p` (proper)** | **Critical** — must test if arg is compile-time constant (not always 0) | Returns 0 unconditionally; kernel uses it in `BUILD_BUG_ON` and optimization paths |
| **`__builtin_types_compatible_p`** | **High** — type comparison without conversion | Not implemented |
| **`__builtin_choose_expr`** | **High** — compile-time conditional expression | Not implemented |
| **`__builtin_add_overflow` / `__builtin_mul_overflow` / `__builtin_sub_overflow`** | **High** — checked arithmetic | Not implemented |
| **`__builtin_assume_aligned`** | **Medium** — alignment assertion | Not implemented |
| **`__builtin_prefetch`** | **Medium** — cache prefetch hint | Not implemented |
| **`__builtin_frame_address(N)`** | **High** — stack unwinding, tracing | Not implemented |
| **`__builtin_return_address(N)`** | **High** — stack unwinding, tracing | Not implemented |
| **`__builtin_extract_return_addr`** | **Low** — pointer authentication | Not implemented |
| **`__builtin_object_size`** | **High** — used for `FORTIFY_SOURCE` buffer checking | Not implemented |
| **`__builtin_dynamic_object_size`** | **Medium** — runtime variant | Not implemented |
| **`__builtin_has_attribute`** | **Low** — attribute introspection | Not implemented |
| **`__builtin_sadd_overflow` etc. (typed variants)** | **Medium** — typed checked arithmetic | Not implemented |
| **`__sync_*` atomics** | **Critical** — legacy GCC atomic builtins | `__sync_fetch_and_add`, `__sync_lock_test_and_set`, `__sync_synchronize`, etc. |
| **`__atomic_*` atomics** | **Critical** — modern GCC atomic builtins | `__atomic_load_n`, `__atomic_store_n`, `__atomic_exchange_n`, `__atomic_compare_exchange_n`, `__atomic_fetch_add`, etc. |
| **`__builtin_ia32_*` x86 intrinsics** | **Low** — SIMD; kernel avoids FPU by default | Not implemented |

---

## 7. Semantic Analysis

### Currently Supported
- Scope management (lexical scoping, nested blocks)
- Undeclared variable detection
- `const` assignment enforcement (on simple variables)
- `volatile` / `restrict` qualifier tracking
- `break`/`continue` outside loop detection
- `case`/`default` outside switch detection
- Recursive expression/statement analysis
- Inline ASM operand validation

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Type inference on expressions** | **Critical** — `analyze_expr` returns no type; the compiler does zero type checking | Without expression types, no implicit conversions, no promotion rules, no assignment compatibility |
| **Integer promotion rules (C11 §6.3.1.1)** | **Critical** — `char`, `short`, `_Bool` must promote to `int` | Not implemented |
| **Usual arithmetic conversions (C11 §6.3.1.8)** | **Critical** — `int + unsigned long` → `unsigned long` | Not implemented |
| **Assignment type compatibility** | **High** — RHS type not checked against LHS | Not implemented |
| **Return type checking** | **High** — return value not checked against declared type | Not implemented |
| **Function call arity/type checking** | **High** — argument count and types not validated | Not implemented |
| **Implicit function declarations** | **Medium** — calling undeclared functions silently allowed | Should at least warn |
| **`const` through pointers** | **Medium** — `const int *p; *p = 5;` not caught | Only simple variable const checked |
| **Lvalue validation** | **Medium** — assignment to non-lvalue not detected | Only const checked |
| **Array-to-pointer decay** | **High** — not modeled; affects type computations | Not implemented |
| **Pointer arithmetic type rules** | **Medium** — no validation of pointer arithmetic | Not checked |
| **Incomplete type detection** | **Medium** — `struct Foo *` before definition of `Foo` | Not checked |
| **Duplicate declarations in same scope** | **Low** — silently allowed | Not checked |
| **`typedef` resolution** | **High** — `Type::Typedef` never resolved to its underlying type | Not implemented |
| **Storage class conflict detection** | **Low** — `static extern int x;` not detected | Not checked |
| **Bitfield width validation** | **Medium** — width > type_bits not caught | Not checked |
| **Initializer shape checking** | **Medium** — initializer list not validated against target | Not checked |
| **Duplicate `case` value detection** | **Low** | Not checked |

---

## 8. IR Representation

### Currently Supported Instructions
`Binary`, `FloatBinary`, `Unary`, `FloatUnary`, `Phi`, `Copy`, `Cast`, `Alloca`, `Load`, `Store`, `GetElementPtr`, `Call`, `IndirectCall`, `VaStart`, `VaEnd`, `VaCopy`, `VaArg`, `InlineAsm`

### Supported Terminators
`Br` (unconditional), `CondBr` (conditional), `Ret`, `Unreachable`

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Volatile flag on Load/Store** | **Critical** — MMIO, hardware registers | `Load`/`Store` have no `is_volatile` field; volatility cannot be preserved through optimization |
| **AtomicLoad / AtomicStore** | **Critical** — `_Atomic`, `__atomic_*` builtins | No atomic memory access instructions |
| **AtomicRMW (read-modify-write)** | **Critical** — `atomic_fetch_add`, `__sync_fetch_and_add` | No instruction |
| **CmpXchg (compare-and-swap)** | **Critical** — `cmpxchg` for lock-free data structures | No instruction |
| **Fence (memory barrier)** | **Critical** — `__sync_synchronize`, `smp_mb()` | No `Fence` instruction |
| **Memory ordering annotations** | **Critical** — relaxed/acquire/release/seq_cst | No ordering enum |
| **IndirectBr (computed goto)** | **High** — `goto *ptr` dispatch tables | No `IndirectBr(Operand, Vec<BlockId>)` terminator |
| **Switch terminator** | **Medium** — switch lowered as CondBr chain; no jump table | No native `Switch` terminator |
| **Select instruction** | **Medium** — branchless conditional `dest = cond ? a : b` | Must use `CondBr` + `Phi` |
| **Aggregate copy / memcpy intrinsic** | **High** — struct assignment, large copies | No bulk memory copy instruction |
| **Intrinsics for bit ops** | **High** — `ctlz`, `cttz`, `popcount`, `bswap` | No intrinsic instructions |
| **Overflow-checking arithmetic** | **Medium** — `__builtin_add_overflow` | No `AddOverflow` instruction |
| **`undef` / `poison` values** | **Low** — for optimization correctness | Not represented |
| **Thread-local storage annotation** | **Medium** — `__thread` / `_Thread_local` on globals | No TLS annotation on IR globals |
| **Debug/source location metadata** | **Medium** — for DWARF generation | No source location tracking on instructions |
| **Calling convention annotations** | **Medium** — per-callsite convention override | Not on `Call`/`IndirectCall` |
| **Address space annotations** | **Low** — `__seg_gs` for per-CPU data | Not represented |

---

## 9. Optimizer

### Currently Supported Passes
1. Mem2Reg (SSA promotion)
2. Algebraic simplification
3. Strength reduction (mul → shift)
4. Copy propagation
5. Load forwarding
6. Common subexpression elimination (CSE)
7. Constant folding & propagation + DCE
8. **Loop interchange** (nested loop stride)
9. **LICM** (loop-invariant code motion)
10. **Software prefetch** (`prefetcht0`, trip ≥ 64)
11. **Auto-vectorization** (SSE2/AVX2: packed, strided gather/scatter, indexed gather/scatter, masked tail; `polyhedral.rs` + `mem_dependence.rs`)
12. Phi removal (SSA → copies)
13. CFG simplification
14. **Block layout** (I-cache; honors `__builtin_expect` / `BranchHint`)

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Volatile-aware optimization** | ~~Critical~~ | ✅ `Load`/`Store` volatile flag; optimizer respects it (Tier 1) |
| **Atomic-aware optimization** | **Critical** — must not reorder across atomics/fences | Atomics exist in IR/codegen; optimizer still needs full fence-aware scheduling |
| **Function inlining** | **High** — `always_inline` | Parsed; inlining pass partial / limited |
| **Loop unrolling** | **Medium** | Not implemented |
| **Tail call optimization** | **Low** | Not implemented |
| **Dead store elimination** | **Medium** | Not implemented |
| **Alias analysis** | **Medium** | Conservative; `mem_dependence` only for vectorization |
| **Interprocedural optimization** | **Low** | Not implemented |
| **`__builtin_expect` utilization** | ~~Low~~ | ✅ `Expr::Expect` → `BranchHint` → block layout (2026-06-02) |
| **Fixed-point iteration** | **Low** | Single-pass pipeline (fold/DCE has inner fixpoint) |

---

## 10. Code Generation & ABI

### Currently Supported
- x86-64 code generation (Intel syntax)
- System V AMD64 calling convention (6 int regs: rdi, rsi, rdx, rcx, r8, r9; 8 SSE regs for floats)
- Windows x64 calling convention
- Register allocation (linear scan)
- Stack frame management (push/pop rbp)
- Integer arithmetic (add, sub, imul, idiv, shifts, bitwise)
- Float arithmetic via SSE (addss/addsd, subss/subsd, mulss/mulsd, divss/divsd)
- Comparison and conditional jumps
- Function calls (direct and indirect)
- Global variable access (RIP-relative addressing)
- Struct member access (base + offset)
- Array indexing
- Pointer arithmetic
- Type casts (int↔float, narrow↔wide integers)

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Struct by-value pass/return (SysV ABI)** | **Critical** — small structs passed in registers per ABI rules | Structs passed as pointers; no ABI-correct register decomposition |
| **`va_arg` codegen** | **Critical** — needed for `printk` | IR instruction exists but codegen emits a stub/comment |
| **Bitfield layout and access** | **Critical** — thousands of bitfields in kernel structs | Parsed but no packing; no shift/mask codegen for access |
| **`static` linkage (non-.globl symbols)** | **Critical** — `static` functions/variables should not be `.globl` | All symbols emitted as `.globl` |
| **Atomic instructions (`lock` prefix)** | **Critical** — `lock xadd`, `lock cmpxchg`, `xchg`, `mfence` | Not implemented |
| **x87 FPU instructions** | **Low** — kernel doesn't use FPU | No x87 codegen; `long double` not possible |
| **128-bit integer operations** | **High** — `__int128` multiply/divide | No 128-bit type or codegen |
| **TLS access (`%fs`/`%gs` segments)** | **High** — per-CPU variables, `current_task` | No TLS codegen |
| **PIC code generation** | **Medium** — `@PLT`, `@GOTPCREL` relocations | Not implemented |
| **Red zone control** | **Critical** — kernel must not use red zone | No `-mno-red-zone` support |
| **Conditional moves (`cmov`)** | **Medium** — branchless code optimization | Not used |
| **`rep movsb` / `rep stosb`** | **High** — efficient memory copy/set | Not implemented; needed for `memcpy`/`memset` |
| **Stack alignment to 16 bytes** | **Medium** — SysV ABI requires 16-byte stack alignment at call | May not be enforced consistently |
| **Double-precision float constant pool** | **Medium** — only f32 constants in pool | `f64` constants stored as `.long` (32-bit hex); should be `.quad` |
| **Callee-saved register spilling** | **Medium** — need to save/restore rbx, r12-r15, rbp | May not be fully correct |
| **Large struct copy (memcpy)** | **High** — struct assignment generates no code for large structs | No memcpy call or inline copy loop |
| **Position-independent code** | **Medium** — kernel modules are PIC | No `@PLT` or `@GOT` |
| **SSE/FPU disable mode** | **Critical** — kernel code must avoid FPU | Float operations unconditionally use SSE |
| **`setcc` instructions** | **Medium** — comparison to boolean without branch | May not be generated |

---

## 11. Assembly Output & ELF

### Currently Supported
- `.intel_syntax noprefix`
- `.data` section for globals and string constants
- `.text` section for functions
- `.globl` on all functions and globals
- `.align` for alignment
- `.section` for custom sections (with ELF flags on Linux)
- `.weak` for weak symbols
- `.asciz` for string literals
- `.byte`, `.short`, `.long`, `.quad`, `.zero` for data
- `.section .init_array` / `.section .fini_array` for constructors/destructors
- `.section .note.GNU-stack` for non-executable stack marking
- Float constant pool (f32 only)

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **`.bss` section** | **Critical** — uninitialized data wastes binary space in `.data` | Zeros explicitly emitted; kernel has many large uninitialized arrays |
| **`.rodata` section** | **Critical** — read-only data must be in `.rodata` | All data goes to `.data`; kernel relies on `.rodata` for const protection |
| **`.type` directives** | **High** — `.type main, @function`, `.type var, @object` | Not emitted; linker/debugger needs these |
| **`.size` directives** | **High** — `.size main, .-main` | Not emitted |
| **`.local` directive** | **Critical** — `static` symbols need `.local` instead of `.globl` | Not emitted; all symbols are global |
| **`.hidden` / `.protected` visibility** | **High** — `__attribute__((visibility(...)))` | Not emitted |
| **`.comm` / `.lcomm`** | **Medium** — common symbols for tentative definitions | Not used |
| **`.p2align`** | **Low** — power-of-2 alignment | Uses `.align` which is arch-dependent |
| **`.cfi_*` directives** | **High** — call frame information for unwinding | No CFI directives emitted; stack unwinding broken |
| **`.file` / `.loc` directives** | **Medium** — DWARF source-level debug info | Not emitted |
| **Section flags/types variety** | **High** — `"ax"`, `"aw"`, `"a"`, `@note`, `@nobits` etc. | Only `"aw"` and `"ax"` used |
| **`.pushsection` / `.popsection`** | **High** — kernel uses these for out-of-line data | Not supported |
| **`.macro` / `.endm`** | **Low** — assembler macros | Not used |
| **`.ifdef` / `.ifndef`** | **Low** — conditional assembly | Not used |
| **`.incbin`** | **Low** — include binary data | Not supported |
| **`.symver`** | **Low** — symbol versioning | Not supported |
| **Double constant pool (f64)** | **Medium** — `.quad` for 64-bit float constants | Only `.long` (f32) in constant pool |

---

## 12. Inline Assembly

### Currently Supported
- Basic `asm("template" : outputs : inputs : clobbers)` syntax
- `%0`, `%1` placeholder substitution with operand values
- Outputs and inputs connected to C variables
- Raw instruction emission

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Constraint letter parsing (`"r"`, `"m"`, `"=r"`, `"+r"`, `"i"`, `"n"`, `"g"`)** | **Critical** — kernel inline asm uses all standard constraints | No constraint parsing; all operands treated the same |
| **Constraint modifiers (`=`, `+`, `&`)** | **Critical** — output-only, read-write, early-clobber | Not implemented |
| **Register class constraints (`"a"` = eax, `"b"` = ebx, `"c"` = ecx, `"d"` = edx, `"S"` = esi, `"D"` = edi)** | **Critical** — kernel requires specific register placement | Not implemented |
| **Memory constraints (`"m"`)** | **Critical** — memory operands for asm | All operands treated as `DWORD PTR` regardless of size |
| **Immediate constraints (`"i"`, `"n"`)** | **High** — pass constants to asm | Not implemented |
| **Matching constraints (`"0"`, `"1"`)** | **High** — same location as output operand | Not implemented |
| **Named operands (`%[name]`)** | **High** — more readable asm templates | Not supported |
| **`asm goto`** | **Critical** — kernel uses extensively for static branches, alternatives | Not supported |
| **`asm volatile`** | **Critical** — prevents asm from being moved/deleted | `volatile` flag accepted but ignored |
| **Clobber list enforcement** | **Critical** — register saving/restoring around asm | Clobber list accepted but completely ignored |
| **`"memory"` clobber** | **Critical** — acts as compiler memory barrier | Accepted but ignored |
| **`"cc"` clobber** | **High** — flags register clobber | Accepted but ignored |
| **Operand size modifiers (`%b0`, `%w0`, `%k0`, `%q0`)** | **High** — byte/word/dword/qword register variants | Not supported |
| **Dialect alternatives (`{att|intel}`)** | **Low** — AT&T/Intel syntax switching | Not supported |
| **Multi-operand output** | **High** — multiple output operands | May work but constraint-dependent |
| **`asm` in AT&T syntax** | **Medium** — kernel asm is AT&T by default | Compiler uses Intel syntax; GCC preprocessed asm will be AT&T |

---

## 13. Linker & Object File Support

### Currently Supported
- Final linking via `gcc <files> -o <output>` 

### Missing

| Gap | Kernel Relevance | Notes |
|-----|-----------------|-------|
| **Object file output (`.o`)** | **Critical** — kernel build system compiles each TU to `.o` | Driver only supports full compile+link or `.s` emit |
| **Linker script support** | **Critical** — kernel uses `vmlinux.lds.S` | No `-T` flag passthrough |
| **`-Wl,` flag passthrough** | **Critical** — kernel passes many linker options | Not supported |
| **`-r` (relocatable link)** | **High** — partial linking for modules | Not supported |
| **`-static`** | **High** — static linking | Not supported |
| **`-nostdlib`** | **Critical** — kernel doesn't use libc | Not supported |
| **LTO (link-time optimization)** | **Low** — `CONFIG_LTO` option | Not supported |

---

## 14. Summary & Priority Tiers

### Tier 0 — Absolute Blockers (Kernel Cannot Begin to Compile)

These must be implemented before any kernel source file can be processed:

1. ~~**`-c` flag (compile to `.o`)**~~ ✅ — DONE: `-c` produces `.o` via `gcc -c`
2. ~~**`-D` / `-U` / `-I` passthrough**~~ ✅ — DONE: forwarded to GCC preprocessor
3. ~~**`-nostdlib` / `-ffreestanding`**~~ ✅ — DONE: forwarded to preprocessor and linker
4. ~~**`-include` flag**~~ ✅ — DONE: `--include` forwarded to GCC preprocessor
5. ~~**Octal integer literals**~~ ✅ — DONE: `0777` → 511, plus binary `0b1010` → 10
6. ~~**Integer suffix preservation**~~ ✅ — DONE: `IntegerSuffix` enum (None/U/L/UL/LL/ULL)
7. ~~**Function prototypes in AST**~~ ✅ — DONE: `FunctionPrototype` struct in `Program.prototypes`
8. ~~**`extern` declarations in AST**~~ ✅ — DONE: `GlobalVar.is_extern`, parsed and stored
9. ~~**`static` linkage**~~ ✅ — DONE: `Function.is_static`/`GlobalVar.is_static`, no `.globl` for static
10. ~~**Forward struct declarations**~~ ✅ — DONE: stored in `Program.forward_structs`

### Tier 1 — Critical Infrastructure (Needed for Most Kernel Files)

11. ~~**Inline asm constraint parsing** — full `"r"`, `"m"`, `"=r"`, `"+r"`, `"i"`, register-class constraints~~ ✅
12. ~~**`asm goto`** — static branches, alternatives framework~~ ✅ (basic asm with goto labels supported)
13. ~~**`asm volatile` enforcement** — `volatile` flag must prevent movement/elimination~~ ✅
14. ~~**Clobber list enforcement** — save/restore registers; respect `"memory"` and `"cc"` clobbers~~ ✅
15. ~~**`.bss` section** — uninitialized data~~ ✅
16. ~~**`.rodata` section** — read-only data~~ ✅
17. ~~**`.type` / `.size` directives** — ELF symbol metadata~~ ✅
18. ~~**Bitfield layout and codegen** — correct packing, shift/mask access~~ ✅
19. ~~**`va_arg` codegen** — needed for `printk` and variadic functions~~ ✅
20. ~~**Struct by-value ABI** — SysV AMD64 struct passing in registers~~ ✅
21. ~~**Volatile semantics in IR** — flag on Load/Store; optimizer must not touch~~ ✅
22. ~~**Atomic operations** — `__sync_*`, `__atomic_*`, `lock` prefix instructions, `mfence`~~ ✅
23. ~~**`__builtin_bswap16/32/64`** — byte swapping for endianness conversion~~ ✅
24. ~~**`__builtin_memcpy` / `__builtin_memset`** — kernel's memory primitives~~ ✅
25. ~~**`__builtin_constant_p` (proper)** — must detect compile-time constants~~ ✅
26. ~~**`-mno-red-zone`** — kernel stack must not use red zone~~ ✅
27. ~~**`-mno-sse` / FPU disable** — kernel code must not emit FPU/SSE instructions~~ ✅
28. ~~**Enum as a type** — `enum E var;` declarations~~ ✅
29. ~~**Qualified pointers** — `const int *` vs `int *const` must be distinct~~ ✅
30. ~~**`.cfi_*` directives** — call frame information for stack unwinding~~ ✅

### Tier 2 — Important for Broad Coverage

31. **`__builtin_types_compatible_p`** — type comparison
32. **`__builtin_choose_expr`** — compile-time conditional
33. ~~**`__builtin_clz/ctz/popcount` runtime codegen**~~ ✅ — `bsr`/`bsf`/`popcnt` in `codegen/call_ops.rs` (2026-06-02)
34. **`__builtin_frame_address` / `__builtin_return_address`** — unwinding
35. **`__builtin_object_size`** — `FORTIFY_SOURCE`
36. **`__builtin_add/sub/mul_overflow`** — checked arithmetic
37. **`__int128` type and operations** — 128-bit arithmetic
38. **Computed goto (`goto *ptr`, `&&label`)** — interpreter dispatch
39. **Anonymous struct/union members** — transparent member access
40. **Complex nested declarators** — `int (*(*fp)(int))(char)` patterns
41. **Designated initializer ranges** — `[0 ... 9] = val` (GCC extension)
42. **`typeof` on types** — `typeof(int *)` not just `typeof(expr)`
43. **Flexible array members** — correct size computation
44. **Type inference in semantic pass** — integer promotions, usual arithmetic conversions
45. **`__attribute__((visibility(...)))`** — ELF symbol visibility
46. **`__attribute__((used))`** — prevent stripping
47. **`__attribute__((alias(...)))`** — symbol aliasing
48. **`__attribute__((noinline))`** — prevent inlining
49. **`__attribute__((cold/hot))`** — code placement
50. **`__attribute__((cleanup(...)))`** — automatic cleanup

### Tier 3 — Full Compatibility

51. **`__attribute__((error/warning))`** — compile-time diagnostics
52. **`__attribute__((format(...)))`** — format checking
53. **`__attribute__((warn_unused_result))`** — `__must_check`
54. **`__attribute__((may_alias))`** — type punning
55. **`__attribute__((no_instrument_function))`** — tracing exclusion
56. **`__attribute__((pure/const))`** — purity annotations
57. **PIC/PIE code generation** — kernel modules
58. **`-mcmodel=kernel`** — upper-half address space
59. **DWARF debug info** — `CONFIG_DEBUG_INFO`
60. **Function inlining pass** — `always_inline` enforcement
61. **Loop unrolling** — LICM/interchange/prefetch/vectorize done; unrolling still missing
62. **Dead store elimination** — performance
63. **Alias analysis** — optimization correctness
64. **Named asm operands (`%[name]`)** — readability
65. **Operand size modifiers (`%b0`, `%w0`)** — register sub-access
66. **Binary integer literals (`0b...`)** — convenience
67. **Linker script support (`-T`)** — vmlinux linking
68. **`-Wl,` passthrough** — linker options
69. **Incomplete array types** — parameter declarations
70. **`_Alignas` on fields** — fine-grained alignment
71. **`.local` visibility** — internal linkage symbols
72. **`.hidden` / `.protected` visibility** — module boundaries
73. **`rep movsb` / `rep stosb`** — efficient memcpy/memset
74. **Conditional moves (`cmov`)** — branchless optimization
75. **`.pushsection` / `.popsection`** — kernel out-of-line annotations
76. **TLS access (`%fs`/`%gs`)** — per-CPU variables
77. **Multi-character constants** — magic numbers
78. **`_Thread_local` storage** — C11 TLS
79. **`_Atomic` types** — C11 atomics
80. **`__auto_type`** — GCC type inference

---

## Appendix A: Current GCC Builtin Coverage

| Builtin | Parser | IR | Codegen | Runtime? |
|---------|--------|----|---------|----------|
| `__builtin_va_start` | ✅ | ✅ | ✅ | ✅ |
| `__builtin_va_end` | ✅ | ✅ | ✅ | ✅ |
| `__builtin_va_copy` | ✅ | ✅ | ✅ | ✅ |
| `__builtin_va_arg` | ✅ | ✅ | ✅ | ✅ |
| `__builtin_unreachable` | ✅ | ✅ | ✅ | N/A |
| `__builtin_trap` | ✅ | ✅ | ✅ | N/A |
| `__builtin_expect` | ✅ `Expect` | ✅ `BranchHint` | N/A | N/A |
| `__builtin_constant_p` | ✅ | ✅ | ✅ | ✅ |
| `__builtin_offsetof` | ✅ | ✅ const | ✅ | N/A |
| `__builtin_clz` | ✅ | ✅ const + Call | ✅ `bsr` | ✅ |
| `__builtin_ctz` | ✅ | ✅ const + Call | ✅ `bsf` | ✅ |
| `__builtin_popcount` | ✅ | ✅ const + Call | ✅ `popcnt` | ✅ |
| `__builtin_abs` | ✅ | ✅ | ✅ | ✅ |

## Appendix B: Current GCC Attribute Coverage

| Attribute | Parsed | Codegen Effect |
|-----------|--------|---------------|
| `packed` | ✅ | ✅ suppresses padding |
| `aligned(N)` | ✅ | ✅ `.align N` |
| `section("name")` | ✅ | ✅ `.section name` |
| `noreturn` | ✅ | ✅ no epilogue |
| `always_inline` | ✅ | ❌ no inlining pass |
| `weak` | ✅ | ✅ `.weak` |
| `unused` | ✅ | ✅ suppresses warnings |
| `constructor` | ✅ | ✅ `.init_array` |
| `destructor` | ✅ | ✅ `.fini_array` |
