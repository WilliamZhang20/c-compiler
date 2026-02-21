// IR (Intermediate Representation) Module
// Lowers AST to SSA-form IR with basic blocks

mod types;
mod lowerer;
mod type_utils;
mod ssa;
mod expressions;
mod lvalue;
mod statements;
mod init_list;
mod mem2reg;
mod ssa_utils;

// Public exports
pub use types::{VarId, BlockId, Operand, Instruction, Terminator, BasicBlock, Function, IRProgram};
pub use lowerer::Lowerer;
pub use mem2reg::mem2reg;
pub use ssa_utils::remove_phis;
pub use ssa_utils::verify_ssa;

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lex;
    use parser::parse_tokens;

    /// Helper: lex + parse + lower → IR program
    fn lower(src: &str) -> IRProgram {
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        lowerer.lower_program(&ast).unwrap()
    }

    /// Helper: get the first (and usually only) function from IR
    fn first_fn(ir: &IRProgram) -> &Function {
        &ir.functions[0]
    }

    /// Helper: flatten all instructions across all blocks
    fn all_instructions(f: &Function) -> Vec<&Instruction> {
        f.blocks.iter().flat_map(|b| b.instructions.iter()).collect()
    }

    // ─── Basic lowering ─────────────────────────────────────────
    #[test]
    fn test_lower_simple_arithmetic() {
        let ir = lower("int main() { int a = 1; int b = 2; return a + b; }");
        assert_eq!(ir.functions.len(), 1);
        let f = first_fn(&ir);
        assert_eq!(f.name, "main");
        assert!(matches!(f.blocks.last().unwrap().terminator, Terminator::Ret(Some(_))));
    }

    #[test]
    fn test_lower_globals() {
        let ir = lower("int g = 10; int main() { return g; }");
        assert_eq!(ir.functions.len(), 1);
        assert_eq!(ir.globals.len(), 1);

        let f = first_fn(&ir);
        let load = f.blocks[0].instructions.iter().find(|i| matches!(i, Instruction::Load { addr: Operand::Global(_), .. }));
        assert!(load.is_some(), "Should use Load from Global");
    }

    #[test]
    fn test_lower_return_constant() {
        let ir = lower("int main() { return 42; }");
        let f = first_fn(&ir);
        assert!(matches!(f.blocks[0].terminator, Terminator::Ret(Some(Operand::Constant(42)))));
    }

    #[test]
    fn test_lower_void_function() {
        let ir = lower("void foo() { } int main() { return 0; }");
        assert_eq!(ir.functions.len(), 2);
        let foo = ir.functions.iter().find(|f| f.name == "foo").unwrap();
        assert!(matches!(foo.return_type, model::Type::Void));
    }

    // ─── Control flow ───────────────────────────────────────────
    #[test]
    fn test_lower_if_else() {
        let ir = lower("int main() { int x = 1; if (x) { return 1; } else { return 0; } }");
        let f = first_fn(&ir);
        // Should have a CondBr somewhere
        let has_cond = f.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondBr { .. }));
        assert!(has_cond, "if-else should produce CondBr terminator");
    }

    #[test]
    fn test_lower_while_loop() {
        let ir = lower("int main() { int i = 0; while (i < 10) { i = i + 1; } return i; }");
        let f = first_fn(&ir);
        assert!(f.blocks.len() >= 3, "while loop needs at least header, body, and exit blocks");
        let has_cond = f.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondBr { .. }));
        assert!(has_cond, "while loop should produce CondBr");
    }

    #[test]
    fn test_lower_for_loop() {
        let ir = lower("int main() { int s = 0; for (int i = 0; i < 5; i = i + 1) { s = s + i; } return s; }");
        let f = first_fn(&ir);
        let has_cond = f.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondBr { .. }));
        assert!(has_cond, "for loop should produce CondBr");
    }

    #[test]
    fn test_lower_do_while() {
        let ir = lower("int main() { int i = 0; do { i = i + 1; } while (i < 5); return i; }");
        let f = first_fn(&ir);
        let has_cond = f.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondBr { .. }));
        assert!(has_cond, "do-while should produce CondBr");
    }

    // ─── Expressions ────────────────────────────────────────────
    #[test]
    fn test_lower_binary_operations() {
        let ir = lower("int main() { int a = 10; int b = 3; return a - b; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_binary = instrs.iter().any(|i| matches!(i, Instruction::Binary { op: model::BinaryOp::Sub, .. }));
        assert!(has_binary, "Should have a subtract instruction");
    }

    #[test]
    fn test_lower_unary_negation() {
        let ir = lower("int main() { int x = 5; return -x; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_unary = instrs.iter().any(|i| matches!(i, Instruction::Unary { op: model::UnaryOp::Minus, .. }));
        assert!(has_unary, "Should have negate unary instruction");
    }

    #[test]
    fn test_lower_comparison() {
        let ir = lower("int main() { int a = 5; int b = 3; return a > b; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_cmp = instrs.iter().any(|i| matches!(i, Instruction::Binary { op: model::BinaryOp::Greater, .. }));
        assert!(has_cmp, "Should have GreaterThan binary instruction");
    }

    #[test]
    fn test_lower_logical_and() {
        let ir = lower("int main() { int a = 1; int b = 1; return a && b; }");
        let f = first_fn(&ir);
        // Logical AND short-circuits, so it creates CondBr blocks
        let has_cond = f.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondBr { .. }));
        assert!(has_cond, "Logical AND should produce short-circuit CondBr");
    }

    // ─── Function calls ─────────────────────────────────────────
    #[test]
    fn test_lower_function_call() {
        let ir = lower("int add(int a, int b) { return a + b; } int main() { return add(1, 2); }");
        assert_eq!(ir.functions.len(), 2);
        let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
        let instrs = all_instructions(main);
        let has_call = instrs.iter().any(|i| matches!(i, Instruction::Call { name, .. } if name == "add"));
        assert!(has_call, "main should call add()");
    }

    #[test]
    fn test_lower_function_with_params() {
        let ir = lower("int identity(int x) { return x; } int main() { return identity(42); }");
        let identity = ir.functions.iter().find(|f| f.name == "identity").unwrap();
        assert_eq!(identity.params.len(), 1, "identity should have 1 parameter");
    }

    // ─── Global strings ─────────────────────────────────────────
    #[test]
    fn test_lower_string_literal() {
        let ir = lower(r#"int main() { char *s = "hello"; return 0; }"#);
        assert!(!ir.global_strings.is_empty(), "String literal should produce global_strings");
    }

    // ─── Structs ────────────────────────────────────────────────
    #[test]
    fn test_lower_struct_definition() {
        let ir = lower("struct Point { int x; int y; }; int main() { struct Point p; p.x = 1; p.y = 2; return p.x + p.y; }");
        assert!(!ir.structs.is_empty(), "Should have struct definitions in IR");
    }

    // ─── Multi-block structure ──────────────────────────────────
    #[test]
    fn test_lower_nested_if() {
        let ir = lower("int main() { int x = 5; if (x > 3) { if (x < 10) { return 1; } } return 0; }");
        let f = first_fn(&ir);
        let cond_count = f.blocks.iter().filter(|b| matches!(b.terminator, Terminator::CondBr { .. })).count();
        assert!(cond_count >= 2, "Nested if should produce at least 2 CondBr terminators");
    }

    #[test]
    fn test_lower_switch() {
        let ir = lower("int main() { int x = 2; switch (x) { case 1: return 1; case 2: return 2; default: return 0; } }");
        let f = first_fn(&ir);
        // Switch turns into a chain of CondBr comparisons
        let cond_count = f.blocks.iter().filter(|b| matches!(b.terminator, Terminator::CondBr { .. })).count();
        assert!(cond_count >= 2, "Switch with 2 cases should produce at least 2 CondBr");
    }

    // ─── Pointers and arrays ────────────────────────────────────
    #[test]
    fn test_lower_pointer_deref() {
        let ir = lower("int main() { int x = 5; int *p = &x; return *p; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_load = instrs.iter().any(|i| matches!(i, Instruction::Load { .. }));
        assert!(has_load, "Pointer dereference should generate Load instruction");
    }

    #[test]
    fn test_lower_array() {
        let ir = lower("int main() { int arr[3]; arr[0] = 1; arr[1] = 2; arr[2] = 3; return arr[1]; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_gep = instrs.iter().any(|i| matches!(i, Instruction::GetElementPtr { .. }));
        assert!(has_gep, "Array indexing should produce GetElementPtr");
    }

    // ─── mem2reg ────────────────────────────────────────────────
    #[test]
    fn test_mem2reg_eliminates_alloca() {
        let ir = lower("int main() { int x = 5; return x; }");
        let mut ir = ir;
        for f in &mut ir.functions {
            mem2reg(f);
        }
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_alloca = instrs.iter().any(|i| matches!(i, Instruction::Alloca { .. }));
        assert!(!has_alloca, "mem2reg should eliminate simple alloca");
    }

    #[test]
    fn test_mem2reg_preserves_semantics() {
        let ir = lower("int main() { int x = 10; int y = x + 5; return y; }");
        let mut ir = ir;
        for f in &mut ir.functions {
            mem2reg(f);
        }
        let f = first_fn(&ir);
        // After mem2reg, return should still reference a valid operand
        let last_block = f.blocks.last().unwrap();
        assert!(matches!(last_block.terminator, Terminator::Ret(Some(_))));
    }

    // ─── Multiple globals ───────────────────────────────────────
    #[test]
    fn test_multiple_globals() {
        let ir = lower("int a = 1; int b = 2; int c = 3; int main() { return a + b + c; }");
        assert_eq!(ir.globals.len(), 3);
    }

    // ─── Enum lowering ──────────────────────────────────────────
    #[test]
    fn test_enum_constants() {
        let ir = lower("enum Color { RED, GREEN, BLUE }; int main() { return GREEN; }");
        let f = first_fn(&ir);
        // GREEN = 1, should appear as a constant
        assert!(matches!(f.blocks[0].terminator, Terminator::Ret(Some(Operand::Constant(1)))));
    }

    // ─── Cast instruction ───────────────────────────────────────
    #[test]
    fn test_cast_expression() {
        let ir = lower("int main() { double d = 3.14; int x = (int)d; return x; }");
        let f = first_fn(&ir);
        let instrs = all_instructions(f);
        let has_cast = instrs.iter().any(|i| matches!(i, Instruction::Cast { .. }));
        assert!(has_cast, "Cast expression should produce Cast instruction");
    }
}
