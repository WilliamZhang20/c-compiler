# Intermediate Representation (IR)

The **IR** (Intermediate Representation) crate defines the data structures used to represent the program in a form that is independent of both the source language (C) and the target machine (Assembly). This abstraction layer facilitates analysis and optimization, allowing the compiler to perform transformations without worrying about syntactic quirks or hardware limitations.

The core of the IR is likely based on a Three-Address Code (TAC) or a Control Flow Graph (CFG) structure. It simplifies the specific statements of C (like `for` loops, `while` loops, and `switch` cases) into a unified set of primitive instructions such as labels, unconditional jumps, conditional branches, and arithmetic operations. This simplification makes it much easier to reason about the flow of the program.

Key components of this crate include the definition of `Instruction`, `BasicBlock`, and `Function` structures. The `Instruction` enum typically covers all supported operations, including arithmetic, logic, memory access, and function calls. The IR also handles variable storage abstraction, often using temporary variables (virtual registers) that are later mapped to physical locations during code generation.

The **Lowering** process, which converts the Semantic AST into this IR, is a critical step managed here or in a related module. It handles the translation of high-level constructs—like breaking down a `for` loop into its initialization, condition check, body, and increment steps—effectively "flattening" the code structure for linear execution.

Future improvements to the IR could include Static Single Assignment (SSA) form, which simplifies many dataflow optimizations. Currently, the IR provides a robust foundation for the optimizer to perform passes like dead code elimination and constant folding before handing off to the machine code generator.
