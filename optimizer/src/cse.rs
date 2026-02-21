use ir::{Function, Instruction, Operand, VarId};
use model::BinaryOp;
use std::collections::HashMap;

/// Common subexpression elimination: eliminate redundant calculations
///
/// Finds expressions that are computed multiple times with the same operands
/// and reuses the first computation instead of recalculating.
pub fn common_subexpression_elimination(func: &mut Function) {
    // Use a compact key representation that can be hashed
    #[derive(Hash, Eq, PartialEq, Clone)]
    struct ExprKey {
        // Encode op type and operands as tuple of integers
        op_id: u8,      // operation identifier
        left_type: u8,  // 0=const, 1=var, 2=global, 3=float
        left_val: i64,  // value or var id
        right_type: u8,
        right_val: i64,
    }
    
    impl ExprKey {
        fn from_binary(op: &BinaryOp, left: &Operand, right: &Operand) -> Self {
            let op_id = match op {
                BinaryOp::Add => 0, BinaryOp::Sub => 1, BinaryOp::Mul => 2, BinaryOp::Div => 3,
                BinaryOp::Mod => 4, BinaryOp::BitwiseAnd => 5, BinaryOp::BitwiseOr => 6,
                BinaryOp::BitwiseXor => 7, BinaryOp::ShiftLeft => 8, BinaryOp::ShiftRight => 9,
                BinaryOp::Less => 10, BinaryOp::LessEqual => 11, BinaryOp::Greater => 12,
                BinaryOp::GreaterEqual => 13, BinaryOp::EqualEqual => 14, BinaryOp::NotEqual => 15,
                BinaryOp::LogicalAnd => 16, BinaryOp::LogicalOr => 17,
                _ => 255, // Other ops not eligible for CSE
            };
            
            let (left_type, left_val) = Self::encode_operand(left);
            let (right_type, right_val) = Self::encode_operand(right);
            
            Self { op_id, left_type, left_val, right_type, right_val }
        }
        
        fn encode_operand(op: &Operand) -> (u8, i64) {
            match op {
                Operand::Constant(c) => (0, *c),
                Operand::Var(v) => (1, v.0 as i64),
                Operand::Global(_) => (2, 0), // Globals need more care
                Operand::FloatConstant(_) => (3, 0), // Skip float constants for now
            }
        }
    }

    let mut expr_map: HashMap<ExprKey, VarId> = HashMap::new();
    let mut var_replacements: HashMap<VarId, VarId> = HashMap::new();

    // Find duplicate expressions within each block
    for block in &func.blocks {
        expr_map.clear(); // Clear per-block to avoid invalid cross-block CSE
        
        for inst in &block.instructions {
            match inst {
                Instruction::Binary {
                    dest,
                    op,
                    left,
                    right,
                } => {
                    // Skip non-pure operations
                    if matches!(op, BinaryOp::Assign | BinaryOp::AddAssign | BinaryOp::SubAssign 
                        | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign
                        | BinaryOp::BitwiseAndAssign | BinaryOp::BitwiseOrAssign 
                        | BinaryOp::BitwiseXorAssign | BinaryOp::ShiftLeftAssign | BinaryOp::ShiftRightAssign) {
                        continue;
                    }
                    
                    let key = if is_commutative(op) {
                        // Canonicalize commutative operations
                        let (l, r) = canonicalize_operands(left, right);
                        ExprKey::from_binary(op, &l, &r)
                    } else {
                        ExprKey::from_binary(op, left, right)
                    };

                    if let Some(&existing_var) = expr_map.get(&key) {
                        // Found a duplicate! Mark for replacement
                        var_replacements.insert(*dest, existing_var);
                    } else {
                        expr_map.insert(key, *dest);
                    }
                }
                _ => {}
            }
        }
    }

    // Replace all uses of eliminated variables
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            replace_in_instruction(inst, &var_replacements);
        }

        // Update terminators
        match &mut block.terminator {
            ir::Terminator::Ret(Some(op)) => {
                replace_in_operand(op, &var_replacements);
            }
            ir::Terminator::CondBr { cond, .. } => {
                replace_in_operand(cond, &var_replacements);
            }
            _ => {}
        }
    }
}

fn replace_in_instruction(inst: &mut Instruction, replacements: &HashMap<VarId, VarId>) {
    match inst {
        Instruction::Binary { left, right, .. } | Instruction::FloatBinary { left, right, .. } => {
            replace_in_operand(left, replacements);
            replace_in_operand(right, replacements);
        }
        Instruction::Unary { src, .. } | Instruction::FloatUnary { src, .. } => {
            replace_in_operand(src, replacements);
        }
        Instruction::Store { addr, src, .. } => {
            replace_in_operand(addr, replacements);
            replace_in_operand(src, replacements);
        }
        Instruction::Copy { src, .. } => {
            replace_in_operand(src, replacements);
        }
        Instruction::GetElementPtr { base, index, .. } => {
            replace_in_operand(base, replacements);
            replace_in_operand(index, replacements);
        }
        Instruction::Call { args, .. } => {
            for arg in args {
                replace_in_operand(arg, replacements);
            }
        }
        Instruction::IndirectCall { func_ptr, args, .. } => {
            replace_in_operand(func_ptr, replacements);
            for arg in args {
                replace_in_operand(arg, replacements);
            }
        }
        _ => {}
    }
}

fn replace_in_operand(op: &mut Operand, replacements: &HashMap<VarId, VarId>) {
    if let Operand::Var(v) = op {
        if let Some(&replacement) = replacements.get(v) {
            *op = Operand::Var(replacement);
        }
    }
}

/// Check if a binary operation is commutative (a op b == b op a)
fn is_commutative(op: &BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Mul
            | BinaryOp::BitwiseAnd
            | BinaryOp::BitwiseOr
            | BinaryOp::BitwiseXor
            | BinaryOp::EqualEqual
            | BinaryOp::NotEqual
    )
}

/// Canonicalize operands for commutative operations (put smaller operand first)
fn canonicalize_operands(left: &Operand, right: &Operand) -> (Operand, Operand) {
    // Order: Constant < Global < Var (by ID)
    match (left, right) {
        (Operand::Constant(_), _) => (left.clone(), right.clone()),
        (_, Operand::Constant(_)) => (right.clone(), left.clone()),
        (Operand::Global(_), Operand::Var(_)) => (left.clone(), right.clone()),
        (Operand::Var(_), Operand::Global(_)) => (right.clone(), left.clone()),
        (Operand::Var(v1), Operand::Var(v2)) if v1.0 <= v2.0 => (left.clone(), right.clone()),
        (Operand::Var(_), Operand::Var(_)) => (right.clone(), left.clone()),
        _ => (left.clone(), right.clone()),
    }
}
