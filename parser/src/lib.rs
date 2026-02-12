// Parser module: Converts a list of tokens into an abstract syntax tree (AST)
//
// Module organization:
// - parser.rs: Core Parser struct and top-level parsing (program, functions, globals)
// - types.rs: Type parsing (int, void, struct, function pointers, etc.)
// - expressions.rs: Expression parsing with precedence climbing
// - statements.rs: Statement parsing (if, while, for, return, etc.)

mod parser;
mod types;
mod expressions;
mod statements;
mod attributes;
mod declarations;
mod utils;

use model::{Program, Token};
use parser::Parser;
use declarations::DeclarationParser;

/// Parse a list of tokens into a Program AST
///
/// # Arguments
/// * `tokens` - Slice of tokens from the lexer
///
/// # Returns
/// * `Ok(Program)` - Successfully parsed program with functions, globals, and structs
/// * `Err(String)` - Parse error with description
pub fn parse_tokens(tokens: &[Token]) -> Result<Program, String> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lex;
    use model::Stmt;

    #[test]
    fn parse_simple_main() {
        let src = "int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
        assert_eq!(program.functions[0].params.len(), 0);
    }

    #[test]
    fn parse_global_variable() {
        let src = "int g_x = 10; int main() { return g_x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.globals.len(), 1);
        assert_eq!(program.globals[0].name, "g_x");
    }

    #[test]
    fn parse_for_loop_decl() {
        let src = "void main() { for (int i = 0; i < 10; i = i + 1) { } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmt = &program.functions[0].body.statements[0];
        if let Stmt::For { init, .. } = stmt {
            assert!(init.is_some());
            let init_box = init.as_ref().unwrap();
            matches!(**init_box, Stmt::Declaration { .. });
        } else {
            panic!("Expected For loop");
        }
    }

    #[test]
    fn parse_function_params() {
        let src = "int add(int a, int b) { return a + b; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].params.len(), 2);
        assert_eq!(program.functions[0].params[0].1, "a");
        assert_eq!(program.functions[0].params[1].1, "b");
    }

    #[test]
    fn parse_variable_declaration() {
        let src = "void main() { int x = 5; int y; y = x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        assert_eq!(stmts.len(), 3);
        matches!(stmts[0], Stmt::Declaration { .. });
        matches!(stmts[1], Stmt::Declaration { .. });
    }

    #[test]
    fn parse_while_loop() {
        let src = "void main() { int x = 0; while (x < 10) { x = x + 1; } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        assert_eq!(stmts.len(), 2);
        matches!(stmts[1], Stmt::While { .. });
    }

    #[test]
    fn parse_for_loop() {
        let src = "void main() { int i; for (i = 0; i < 10; i = i + 1) { return i; } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        matches!(program.functions[0].body.statements[1], Stmt::For { .. });
    }

    #[test]
    fn parse_logical_ops() {
        let src = "int main() { return (1 && 0) || !1; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(matches!(program.functions[0].body.statements[0], Stmt::Return(Some(_))));
    }

    #[test]
    fn parse_relational_ops() {
        let src = "int main() { return 1 <= 2 && 3 != 4; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(matches!(program.functions[0].body.statements[0], Stmt::Return(Some(_))));
    }

    #[test]
    fn parse_2d_array_decl() {
        let src = "int main() { int arr[2][2]; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens);
        assert!(program.is_ok(), "2D array declaration failed to parse");
    }

    #[test]
    fn test_header_tolerance() {
        let src = "typedef int my_int; struct foo { int x; }; int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].name, "main");
    }
}
