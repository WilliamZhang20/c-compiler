# Code Generator

The **Codegen** crate is the final compilation stage. It translates the optimized, phi-free IR into x86-64 assembly text in Intel syntax, ready for `gcc` to assemble and link.

**Public API**: `Codegen::new()` then `codegen.gen_program(&ir_program) -> String`

## How it works

Code generation proceeds in three layers:

1. **Register allocation** — graph-coloring allocator assigns physical registers to SSA variables, with spill slots for overflow
2. **IR → x86 instruction selection** — each IR instruction maps to one or more `X86Instr` values
3. **Peephole optimization + emission** — simplify the instruction stream, then serialize to Intel-syntax assembly text

The codegen targets the **System V AMD64 ABI** (Linux) by default. Windows x64 support is also implemented via a calling convention abstraction layer.

## Source files

### `lib.rs` — Program-level driver
The `Codegen` struct holds shared state: struct/union definitions, float constant pool, function return type map, and target configuration. `gen_program()` emits:
1. `.data` section — global strings (`.asciz`), global variables with alignment, optional custom `section` directives. Extern globals (`is_extern`) with no initializer are skipped.
2. `.text` section — one `FunctionGenerator` per IR function. Static functions/globals omit `.globl` for internal linkage.
3. Float constant data (labeled `.LC*` values)
4. `.note.GNU-stack` marker for non-executable stacks
5. `.init_array` / `.fini_array` entries for `__attribute__((constructor/destructor))`

### `function.rs` — Per-function code generation
`FunctionGenerator` manages stack layout, register allocation results, var-to-operand mappings, and alloca buffer tracking. Key operations:

- **`gen_function()`** — runs register allocation, emits prologue (callee-saved pushes, frame pointer, stack reservation with **backpatch placeholder** for late spill slots), parameter moves from ABI registers with **cycle detection** to avoid overwrites, block-by-block instruction emission, and epilogue
- **`gen_instr()`** — dispatches each IR instruction to the appropriate generator
- **`var_to_op()` / `operand_to_op()`** — translates IR operands to `X86Operand` using register allocation results, stack slots, and alloca buffers
- **Cast handling** — int↔float (`cvtsi2ss`/`cvttss2si`), pointer casts, 32/64-bit width mismatches

Stack frame calculation accounts for locals, callee-saved registers, shadow space (Windows), and stack-passed call arguments (>6 args).

### `instructions.rs` — Integer/pointer arithmetic
`gen_binary_op()` handles `Add`, `Sub`, `Mul`, `Div`/`Mod` (via `cdq`/`cqo` + `idiv`), all six comparisons (`cmp` + `set*`), bitwise ops, and shifts. Automatically selects 32-bit vs 64-bit register variants based on operand types. Optimizes the case where the destination already holds one operand.

### `float_ops.rs` — SSE floating-point
`gen_float_binary_op()` emits `addss`/`subss`/`mulss`/`divss` for arithmetic, `ucomiss` + `set*` for comparisons, with automatic `cvtsi2ss` for mixed int/float operands. `gen_float_unary_op()` handles negation (sign-bit XOR via `xorps`) and logical not.

### `memory_ops.rs` — Load, store, GEP
`gen_load()` emits correctly-sized memory reads: `BYTE` (with `movsx`/`movzx`), `DWORD`, `QWORD`, or `movss` for floats — from allocas, globals (RIP-relative), or general pointers. `gen_store()` writes with matching size logic. `gen_gep()` computes `base + index * element_size` using `imul` + `add`/`lea`.

### `call_ops.rs` — Function calls
`gen_call()` and `gen_indirect_call()` route integer arguments to GP registers and float arguments to XMM registers per the active ABI, spilling excess to the stack. Return values move from `RAX` (int) or `XMM0` (float). Handles `Alloca` buffers (passes address via `LEA`), global operands, and variadic setup. Indirect calls stash the function pointer in `R10` before argument setup.

### `calling_convention.rs` — ABI abstraction
The `CallingConvention` trait exposes parameter registers, return registers, shadow space, and callee-saved sets. Two implementations:

| | System V | Windows x64 |
|---|---|---|
| GP param regs | RDI, RSI, RDX, RCX, R8, R9 | RCX, RDX, R8, R9 |
| XMM param regs | XMM0–XMM7 | XMM0–XMM3 |
| Shadow space | 0 bytes | 32 bytes |
| Callee-saved | RBX, R12–R15 | RBX, RSI, RDI, R12–R15 |

`host_convention()` selects the correct one at compile time.

### `regalloc.rs` — Graph-coloring register allocator
`allocate_registers()` runs these phases:
1. Compute live intervals (via `liveness.rs`)
2. Build interference graph from overlapping intervals
3. Collect copy and parameter hints for coalescing
4. Determine call-crossing variables (prefer callee-saved registers)
5. Color greedily: parameter hint → copy hint → caller-saved → callee-saved → any

9 GP registers are allocatable (`RBX`, `RSI`, `RDI`, `R8`, `R9`, `R12`–`R15`). `RAX`, `RCX`, `RDX`, `R10`, `R11` are reserved as scratch. Variables that don't receive a register spill to stack slots.

### `liveness.rs` — Dataflow liveness analysis
`compute_live_intervals()` performs iterative dataflow: per-block use/def sets, then `live_in(B) = use(B) ∪ (live_out(B) - def(B))` and `live_out(B) = ∪ live_in(S)` to fixed point. Handles CFG back-edges correctly.

### `globals.rs` — Global initializer emission
Emits `.byte`/`.long`/`.quad`/`.float` directives for global variable initializers. Handles array and struct initializer lists with designated initializers, padding, alignment, and nested structs.

### `peephole.rs` — Assembly-level peephole optimizations
Applied after instruction selection:
- **Jump chain elimination** — transitive jump resolution, dead label+jump removal
- **Comparison fusion** — multi-instruction `cmp`/`set`/`test`/`jcc` → direct `cmp` + `jcc`
- **Redundant move removal** — `mov reg, reg` elimination, `mov reg, X; mov Y, reg` → `mov Y, X`
- **Identity removal** — `add/sub X, 0`, `imul X, 1`
- **LEA formation** — `mov reg, imm; add reg, reg2` → `lea reg, [reg2 + imm]`

Uses conservative `is_reg_used_after()` liveness checks.

### `types.rs` — Type size/alignment calculator
`TypeCalculator` computes byte sizes for all C types including arrays, structs (with field padding and `__attribute__((packed))`), and unions (max-field-size).

### `x86.rs` — x86-64 instruction representation
- `X86Reg` — 42 register variants (64/32/8-bit GP + XMM0–XMM7)
- `X86Operand` — register, memory (byte/dword/qword/float), immediate, label, RIP-relative
- `X86Instr` — 40-variant enum modeling integer ALU, SSE float, control flow, stack, sign-extension, raw inline assembly
- `emit_asm(instrs) -> String` — serializes to Intel-syntax assembly text

### `control_flow.rs` / `inline_asm.rs`
Extracted helpers for terminator code generation (`Ret`, `Br`, `CondBr` with phi resolution) and inline assembly template expansion.
