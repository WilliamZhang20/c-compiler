# Semantic Analyzer

The **Semantic Analyzer** crate is responsible for verifying the semantic correctness of the parsed Abstract Syntax Tree (AST). While the parser ensures the code structure adheres to the grammar, the semantic analyzer checks for logical consistency, type correctness, and proper scoping rules.

A primary function of this component is **Type Checking**. It ensures that operations are performed on compatible types—for example, preventing the addition of a pointer to a struct or ensuring that function arguments match the declared parameters. It also tracks variable declarations and usages to catch errors like "variable not declared" or "redefinition of symbol."

This crate maintains a **Symbol Table** (or environment) that tracks the scope of variables and functions. It handles block scoping rules (variables declared inside `{ ... }` are not visible outside), function scoping, and global definitions. This ensures that identifiers are correctly resolved to their definitions.

The output of the semantic analysis is typically an annotated AST or a verified AST, which is then safe to be lowered into Intermediate Representation (IR). If semantic errors are found, the analyzer reports them with helpful messages, stopping the compilation before invalid code can reach the code generator.

Key checks performed include:
*   Use of undeclared variables.
*   Type mismatches in assignments and expressions.
*   Correct return types in functions.
*   Duplicate declarations in the same scope.
*   Control flow validity (e.g., `break` must be inside a loop).
