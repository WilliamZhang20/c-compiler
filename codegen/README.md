# Code Generator

The **Codegen** crate is the final stage of the compilation pipeline, responsible for translating the optimized Intermediate Representation (IR) into platform-specific assembly code. It targets the x86-64 architecture, generating assembly that uses the System V AMD64 ABI, which is standard on Linux and widely supported by tools like GCC and Clang.

This component takes the high-level IR instructions—such as binary operations, memory loads/stores, and control flow jumps—and maps them to concrete assembly instructions. It handles the complexities of lowering abstract types and operations into machine-level constructs, ensuring that the semantics of the C program are preserved in the final executable.

A key implementation detail of this codegen is its management of the stack frame and function calling conventions. It properly sets up the stack for local variables, handles parameter passing via registers (rdi, rsi, rdx, etc.) and stack slots, and ensures proper register preservation across function calls. This allows the generated code to interoperate seamlessly with C system libraries.

The output of this crate is an assembly string (typically saved with a `.s` extension) that can be assembled and linked by an external assembler/linker, such as `gcc`. This design keeps the compiler focused on code generation logic while leveraging existing, robust tools for the final binary creation.

Extensions to this crate would likely involve adding support for instruction selection optimizations, such as better register allocation (currently relying on stack-heavy code or simple allocation) or peephole optimizations to reduce the number of generated instructions.
