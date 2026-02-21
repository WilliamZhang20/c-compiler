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

    #[test]
    fn test_lower_simple_arithmetic() {
        let src = "int main() { int a = 1; int b = 2; return a + b; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();
        
        assert_eq!(ir.functions.len(), 1);
        let f = &ir.functions[0];
        assert_eq!(f.name, "main");
        
        // entry block should have 2 copies and 1 binary op and a return
        let entry = &f.blocks[0];
        assert!(matches!(entry.terminator, Terminator::Ret(Some(Operand::Var(_)))));
    }

    #[test]
    fn test_lower_globals() {
        let src = "int g = 10; int main() { return g; }";
        let tokens = lex(src).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        let mut lowerer = Lowerer::new();
        let ir = lowerer.lower_program(&ast).unwrap();
        
        assert_eq!(ir.functions.len(), 1);
        assert_eq!(ir.globals.len(), 1);
        
        let f = &ir.functions[0];
        // Should NOT have a Copy instruction to load the global address (optimized out)
        let copy = f.blocks[0].instructions.iter().find(|i| matches!(i, Instruction::Copy { src: Operand::Global(_), .. }));
        assert!(!copy.is_some());
        // Should have a Load instruction using the global address directly
        let load = f.blocks[0].instructions.iter().find(|i| matches!(i, Instruction::Load { addr: Operand::Global(_), .. }));
        assert!(load.is_some());
    }
}
