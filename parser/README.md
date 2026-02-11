# Parser

The **Parser** crate analyzes the token stream produced by the lexer and constructs an Abstract Syntax Tree (AST) that represents the hierarchical structure of the C program. It implements a Recursive Descent Parser, a top-down parsing strategy where each non-terminal symbol in the grammar corresponds to a function in the parser (e.g., `parse_statement`, `parse_expression`, `parse_function`).

This component handles the complex grammar rules of the C language, including operator precedence, associativity, and the various statement types like `if`, `while`, `for`, `return`, and block scopes. It converts linear sequences of tokens into a structured tree where nodes represent constructs like `FunctionDefinition`, `VariableDeclaration`, `BinaryExpression`, and `IfStatement`.

A key responsibility of the parser is to ensure syntactical correctness. If the source code violates the grammar rules (e.g., missing semicolon, mismatched parentheses, invalid declaration), the parser detects this and reports a descriptive error. It also handles certain ambiguities in C syntax by using lookahead or backtracking where necessary.

The output of the parser is a `Program` structure containing a list of global declarations, functions, and type definitions. This AST is then passed to the Semantic Analyzer for type checking and further validation. The parser does not perform type checking itself but ensures that the code follows the structural rules of the language.

The design relies on the `Token` definitions from the `model` crate and produces AST nodes also defined in `model`. By isolating the parsing logic here, the compiler maintains a clear separation of concerns, making the grammar rules easier to maintain and extend.
