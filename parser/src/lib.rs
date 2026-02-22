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

    // ─── Type parsing tests ────────────────────────────────────
    #[test]
    fn parse_unsigned_int() {
        let src = "int main() { unsigned int x = 1; return x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, .. } = &stmts[0] {
            assert_eq!(*r#type, model::Type::UnsignedInt);
        } else {
            panic!("Expected Declaration");
        }
    }

    #[test]
    fn parse_long_long() {
        let src = "int main() { long long x = 0; return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, .. } = &stmts[0] {
            assert_eq!(*r#type, model::Type::LongLong);
        } else {
            panic!("Expected Declaration");
        }
    }

    #[test]
    fn parse_unsigned_long_long() {
        let src = "int main() { unsigned long long x = 0; return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, .. } = &stmts[0] {
            assert_eq!(*r#type, model::Type::UnsignedLongLong);
        } else {
            panic!("Expected Declaration");
        }
    }

    #[test]
    fn parse_bool_type() {
        let src = "int main() { _Bool b = 1; return b; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, .. } = &stmts[0] {
            assert_eq!(*r#type, model::Type::Bool);
        } else {
            panic!("Expected Declaration");
        }
    }

    #[test]
    fn parse_pointer_type() {
        let src = "int main() { int *p; return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, .. } = &stmts[0] {
            assert_eq!(*r#type, model::Type::Pointer(Box::new(model::Type::Int)));
        } else {
            panic!("Expected Declaration");
        }
    }

    // ─── Expression parsing tests ───────────────────────────────
    #[test]
    fn parse_ternary() {
        let src = "int main() { return 1 ? 2 : 3; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[0] {
            assert!(matches!(expr, model::Expr::Conditional { .. }));
        } else {
            panic!("Expected Return with Conditional");
        }
    }

    #[test]
    fn parse_sizeof_type() {
        let src = "int main() { return sizeof(int); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[0] {
            assert!(matches!(expr, model::Expr::SizeOf(model::Type::Int)));
        } else {
            panic!("Expected Return with SizeOf");
        }
    }

    #[test]
    fn parse_alignof() {
        let src = "int main() { return _Alignof(int); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[0] {
            assert!(matches!(expr, model::Expr::AlignOf(model::Type::Int)));
        } else {
            panic!("Expected Return with AlignOf");
        }
    }

    #[test]
    fn parse_cast() {
        let src = "int main() { return (int)3.14; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[0] {
            assert!(matches!(expr, model::Expr::Cast(model::Type::Int, _)));
        } else {
            panic!("Expected Return with Cast");
        }
    }

    #[test]
    fn parse_comma_expression() {
        let src = "int main() { return (1, 2, 3); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[0] {
            if let model::Expr::Comma(exprs) = expr {
                assert_eq!(exprs.len(), 3);
            } else {
                panic!("Expected Comma, got {:?}", expr);
            }
        } else {
            panic!("Expected Return");
        }
    }

    #[test]
    fn parse_index_expression() {
        let src = "int main() { int arr[3]; return arr[0]; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(expr)) = &program.functions[0].body.statements[1] {
            assert!(matches!(expr, model::Expr::Index { .. }));
        } else {
            panic!("Expected Return with Index");
        }
    }

    #[test]
    fn parse_member_access() {
        let src = "struct S { int x; }; int main() { struct S s; return s.x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        // Find the return statement
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Return(Some(expr)) = &stmts[stmts.len() - 1] {
            assert!(matches!(expr, model::Expr::Member { member, .. } if member == "x"));
        } else {
            panic!("Expected Return with Member");
        }
    }

    // ─── Statement parsing tests ────────────────────────────────
    #[test]
    fn parse_do_while() {
        let src = "void main() { int x = 0; do { x = x + 1; } while (x < 5); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(matches!(program.functions[0].body.statements[1], Stmt::DoWhile { .. }));
    }

    #[test]
    fn parse_switch() {
        let src = "void main() { int x = 1; switch (x) { case 1: break; default: break; } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(matches!(program.functions[0].body.statements[1], Stmt::Switch { .. }));
    }

    #[test]
    fn parse_goto_and_label() {
        let src = "void main() { goto end; end: return; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        assert!(matches!(&stmts[0], Stmt::Goto(label) if label == "end"));
        assert!(matches!(&stmts[1], Stmt::Label(label) if label == "end"));
    }

    #[test]
    fn parse_nested_blocks() {
        let src = "void main() { { int x = 1; { int y = 2; } } }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(matches!(program.functions[0].body.statements[0], Stmt::Block(_)));
    }

    // ─── Declaration tests ──────────────────────────────────────
    #[test]
    fn parse_multi_variable_declaration() {
        let src = "void main() { int a = 1, b = 2, c = 3; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        // Multi-decl should produce a MultiDecl node or flatten
        assert!(!stmts.is_empty());
    }

    #[test]
    fn parse_string_literal_expr() {
        let src = r#"int main() { char *s = "hello"; return 0; }"#;
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { init: Some(expr), .. } = &stmts[0] {
            assert!(matches!(expr, model::Expr::StringLiteral(_)));
        } else {
            panic!("Expected Declaration with StringLiteral init");
        }
    }

    #[test]
    fn parse_enum_definition() {
        let src = "enum Color { RED, GREEN, BLUE }; int main() { return RED; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.enums.len(), 1);
        assert_eq!(program.enums[0].name, "Color");
        assert_eq!(program.enums[0].constants.len(), 3);
    }

    #[test]
    fn parse_struct_definition() {
        let src = "struct Point { int x; int y; }; int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.structs.len(), 1);
        assert_eq!(program.structs[0].name, "Point");
        assert_eq!(program.structs[0].fields.len(), 2);
    }

    #[test]
    fn parse_function_pointer_local() {
        // Function pointer as local variable (parser supports this)
        let src = "int main() { int (*fp)(int, int); return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let stmts = &program.functions[0].body.statements;
        if let Stmt::Declaration { r#type, name, .. } = &stmts[0] {
            assert_eq!(name, "fp");
            assert!(matches!(r#type, model::Type::FunctionPointer { .. }));
        } else {
            panic!("Expected FunctionPointer declaration");
        }
    }

    #[test]
    fn parse_const_qualifier() {
        let src = "int main() { const int x = 5; return x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Declaration { qualifiers, .. } = &program.functions[0].body.statements[0] {
            assert!(qualifiers.is_const);
        } else {
            panic!("Expected Declaration with const qualifier");
        }
    }

    #[test]
    fn parse_typedef_usage() {
        let src = "typedef int my_int; int main() { my_int x = 42; return x; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Declaration { r#type, .. } = &program.functions[0].body.statements[0] {
            assert_eq!(*r#type, model::Type::Typedef("my_int".to_string()));
        } else {
            panic!("Expected Declaration with typedef type");
        }
    }

    // ─── Attribute tests ────────────────────────────────────────
    #[test]
    fn parse_packed_attribute() {
        let src = "struct __attribute__((packed)) S { int x; char y; }; int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(program.structs[0].attributes.contains(&model::Attribute::Packed));
    }

    #[test]
    fn parse_constructor_attribute() {
        let src = "__attribute__((constructor)) void init() { } int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let init_fn = program.functions.iter().find(|f| f.name == "init").unwrap();
        assert!(init_fn.attributes.contains(&model::Attribute::Constructor));
    }

    // ─── Edge cases ─────────────────────────────────────────────
    #[test]
    fn parse_empty_function_body() {
        let src = "void noop() { }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions[0].body.statements.len(), 0);
    }

    #[test]
    fn parse_multiple_functions() {
        let src = "int foo() { return 1; } int bar() { return 2; } int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.functions.len(), 3);
    }

    #[test]
    fn parse_nested_function_calls() {
        let src = "int f(int x) { return x; } int main() { return f(f(1)); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        if let Stmt::Return(Some(model::Expr::Call { args, .. })) = &program.functions[1].body.statements[0] {
            assert!(matches!(&args[0], model::Expr::Call { .. }));
        } else {
            panic!("Expected nested Call");
        }
    }

    #[test]
    fn parse_function_prototype() {
        let src = "int compute(int a, int b); int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert_eq!(program.prototypes.len(), 1);
        assert_eq!(program.prototypes[0].name, "compute");
        assert_eq!(program.prototypes[0].params.len(), 2);
        assert_eq!(program.functions.len(), 1);
    }

    #[test]
    fn parse_extern_global() {
        let src = "extern int external_val; int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(program.globals.iter().any(|g| g.name == "external_val" && g.is_extern));
    }

    #[test]
    fn parse_static_function() {
        let src = "static int helper(int x) { return x + 1; } int main() { return helper(0); }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let helper = program.functions.iter().find(|f| f.name == "helper").unwrap();
        assert!(helper.is_static);
        let main_fn = program.functions.iter().find(|f| f.name == "main").unwrap();
        assert!(!main_fn.is_static);
    }

    #[test]
    fn parse_static_global() {
        let src = "static int counter = 5; int main() { return counter; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        let counter = program.globals.iter().find(|g| g.name == "counter").unwrap();
        assert!(counter.is_static);
    }

    #[test]
    fn parse_forward_struct() {
        let src = "struct opaque; int main() { return 0; }";
        let tokens = lex(src).unwrap();
        let program = parse_tokens(&tokens).unwrap();
        assert!(program.forward_structs.contains(&"opaque".to_string()));
    }
}
