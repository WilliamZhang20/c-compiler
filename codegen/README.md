# Code Generator

The **Codegen** crate is the final stage of the compilation pipeline. It translates the optimized, phi-free IR into x86-64 assembly text (Intel syntax) that can be assembled and linked by an external tool such as `gcc`. The public entry point is `Codegen::gen_program(prog) -> String`.

## Architecture

Code generation proceeds in two layers:

1. **IR → internal `X86Instr` representation** — each IR instruction is lowered to one or more `X86Instr` values, a typed enum that models the x86-64 instruction set.
2. **`X86Instr` → assembly text** — the `emit_asm()` function serializes the instruction buffer to a string in Intel syntax.

Between these layers, a peephole optimizer simplifies the instruction stream. Register allocation is performed per-function before instruction selection begins. The codegen targets the **System V AMD64 ABI** (Linux) by default but abstracts calling conventions to also support Windows x64.

## Source Files

### `lib.rs`
Program-level driver. The `Codegen` struct holds shared state (float constants, target configuration). `gen_program()` emits the `.data` section (global strings as `.asciz`, global variables with alignment and optional custom section directives), then the `.text` section (one `FunctionGenerator` per IR function), followed by float constant data and a `.note.GNU-stack` marker for non-executable stacks.

### `function.rs`
Per-function code generation orchestrator and the largest file in the module. `FunctionGenerator` manages stack layout, register-allocation results, variable-to-operand mappings, and alloca buffer tracking. Key operations:

- **`gen_function()`** — runs register allocation, emits the prologue (callee-saved pushes, frame pointer setup, stack reservation), moves parameters from ABI registers to their assigned locations (with cycle detection to avoid overwrites), walks all blocks emitting instructions and terminators, and appends the epilogue.
- **`gen_instr()`** — dispatches each IR instruction to the appropriate specialized generator (see below).
- **`gen_terminator()`** — handles `Ret` (with callee-saved restore), `Br` (unconditional jump), and `CondBr` (test + conditional jump). Both branch variants resolve remaining phi copies at block transitions via `resolve_phis()`.
- **`var_to_op()` / `operand_to_op()`** — translates IR variables and operands to `X86Operand`, consulting register allocation, stack slots, and alloca buffers.
- **Cast handling** — covers int↔float (`cvtsi2ss`/`cvttss2si`), pointer casts, and 32/64-bit width mismatches.

### `instructions.rs`
Integer/pointer arithmetic and logic. `gen_binary_op()` handles `Add`, `Sub`, `Mul`, `Div`/`Mod` (via `cdq`/`cqo` + `idiv`), all six comparisons (`cmp` + `set*`), bitwise ops, and shifts. `gen_unary_op()` covers negation, bitwise not, logical not, and identity. Automatically selects 32-bit vs 64-bit register variants based on operand types and optimizes the common case where the destination already holds one operand.

### `float_ops.rs`
SSE scalar single-precision code generation. `gen_float_binary_op()` emits `addss`/`subss`/`mulss`/`divss` for arithmetic and `ucomiss` + `set*` for comparisons, with automatic `cvtsi2ss` for integer operands. `gen_float_unary_op()` handles negation (sign-bit XOR via `xorps`) and logical not.

### `memory_ops.rs`
Load, store, and address arithmetic. `gen_load()` emits correctly-sized memory reads — `BYTE` (with `movsx`/`movzx`), `DWORD`, `QWORD`, or `movss` for floats — from allocas, globals (RIP-relative), or general pointers. `gen_store()` writes values to memory with matching size logic. `gen_gep()` computes `base + index * element_size` using `imul` and `add`/`lea`, supporting alloca, global, and pointer base operands.

### `call_ops.rs`
Function call code generation. `gen_call()` and `gen_indirect_call()` route integer arguments to GP registers and float arguments to XMM registers per the active calling convention, spilling excess arguments to the stack. Return values are moved from `RAX` (int) or `XMM0` (float) to the destination. Handles `Alloca` buffers (passes address via `LEA`), global operands, and variadic call setup. Indirect calls stash the function pointer in `R10` before argument setup.

### `calling_convention.rs`
Platform ABI abstraction. The `CallingConvention` trait exposes parameter registers, return registers, shadow space size, and callee-saved register sets. Two implementations: `SystemVConvention` (6 GP + 8 XMM param regs, no shadow space) and `WindowsX64Convention` (4 GP + 4 XMM, 32-byte shadow space). `host_convention()` selects the correct one at compile time.

### `regalloc.rs`
**Graph-coloring register allocator.** `allocate_registers()` runs these phases:

1. Compute live intervals (def/use positions for each variable).
2. Build an interference graph from overlapping intervals.
3. Collect copy and parameter hints for coalescing.
4. Determine which variables are live across calls (must prefer callee-saved registers).
5. Color the graph greedily: try parameter hint → copy hint → caller-saved → callee-saved → any available.

Nine GP registers are allocatable (`RBX, RSI, RDI, R8, R9, R12–R15`); `RAX`, `RCX`, `RDX`, `R10`, `R11` are reserved as scratch. Variables that don't receive a register are spilled to stack slots.

### `peephole.rs`
Assembly-level peephole optimizations applied after instruction selection:

- **Jump chain elimination** — resolves transitive jumps and removes dead label+jump pairs.
- **Comparison fusion** — collapses multi-instruction compare-set-test-jump patterns into direct `cmp` + `jcc`.
- **Redundant move removal** — eliminates `mov reg, reg` and coalesces `mov reg, X; mov Y, reg` into `mov Y, X` when legal.
- **Identity operation removal** — drops `add/sub X, 0` and `imul X, 1`.
- **LEA formation** — converts `mov reg, imm; add reg, reg2` into `lea reg, [reg2 + imm]`.

Uses a conservative `is_reg_used_after()` liveness check to ensure replaced registers are dead.

### `types.rs`
Type size and alignment calculation. `TypeCalculator` computes byte sizes for all C types including arrays, structs (with field padding and `__attribute__((packed))` support), and unions (max-field-size). Used by stack allocation, GEP scaling, and struct layout throughout the codegen.

### `x86.rs`
The x86-64 instruction representation layer. Defines:

- **`X86Reg`** — 44 register variants covering 64-bit, 32-bit, and 8-bit GP registers plus `XMM0`–`XMM7`.
- **`X86Operand`** — register, memory (byte/dword/qword/float), immediate, label, and RIP-relative addressing modes.
- **`X86Instr`** — 39-variant enum modeling integer ALU, SSE float, control flow, stack, sign-extension, and raw inline assembly instructions.
- **`emit_asm()`** — serializes a `Vec<X86Instr>` to Intel-syntax assembly text.
