# Lexer

The **Lexer** (Lexical Analyzer) crate is the first stage of the compilation process. It takes the raw source code string and converts it into a stream of tokens, which are the fundamental building blocks of the language syntax. This includes identifying keywords (like `int`, `return`, `if`), identifiers (variable names), literals (numbers, characters), and operators (`+`, `-`, `=`, etc.).

The lexer works by scanning the input character by character and grouping them into meaningful units. It also handles whitespace and comments, filtering them out so that the parser can focus on the significant code structure. For specialized tokens like escape sequences in strings or multi-character operators (e.g., `==`, `!=`, `&&`), the lexer employs specific logic to correctly distinguish them from single characters.

A `Token` enum defined in the `model` crate is used to represent the different token types. The main entry point is typically a `lex` function that returns a `Result<Vec<Token>, String>`, providing error messages with line numbers if an invalid character is encountered. This makes it easier for users to locate syntax errors early in the compilation process.

One of the challenges handled here is the contextual meaning of certain characters in C, such as the asterisk `*` (which can be a pointer declaration, dereference, or multiplication) or the ampersand `&` (address-of or bitwise AND). The lexer identifies the token, while the parser later determines the context.

This component is designed to be fast and robust, capable of handling large source files efficiently. It serves as the foundation for the parser, ensuring that syntactically valid streams of tokens are passed downstream for structural analysis.
