use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::{BinaryOp, UnaryOp, Type};
use ir::{VarId, Operand};
use std::collections::HashMap;

/// Instruction generation for arithmetic and logical operations
pub struct InstructionGenerator;

impl InstructionGenerator {
    pub fn gen_binary_op(
        asm: &mut Vec<X86Instr>,
        _dest: VarId,
        op: &BinaryOp,
        l_op: X86Operand,
        r_op: X86Operand,
        d_op: X86Operand,
    ) {
        match op {
            BinaryOp::Add => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::Add(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Sub => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::Sub(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Mul => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::Imul(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::Div => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                asm.push(X86Instr::Cqto);
                if let X86Operand::Imm(_) = r_op {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    asm.push(X86Instr::Idiv(X86Operand::Reg(X86Reg::Rcx)));
                } else {
                    asm.push(X86Instr::Idiv(r_op));
                }
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            BinaryOp::Mod => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                asm.push(X86Instr::Cqto);
                if let X86Operand::Imm(_) = r_op {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    asm.push(X86Instr::Idiv(X86Operand::Reg(X86Reg::Rcx)));
                } else {
                    asm.push(X86Instr::Idiv(r_op));
                }
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rdx)));
            }
            BinaryOp::EqualEqual | BinaryOp::NotEqual | BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Rax), r_op));
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                let cond = match op {
                    BinaryOp::EqualEqual => "e",
                    BinaryOp::NotEqual => "ne",
                    BinaryOp::Less => "l",
                    BinaryOp::LessEqual => "le",
                    BinaryOp::Greater => "g",
                    BinaryOp::GreaterEqual => "ge",
                    _ => unreachable!(),
                };
                asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            BinaryOp::BitwiseAnd => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::And(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::BitwiseOr => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::Or(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Or(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::BitwiseXor => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    asm.push(X86Instr::Mov(d_op.clone(), l_op));
                    asm.push(X86Instr::Xor(d_op, r_op));
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Xor(X86Operand::Reg(X86Reg::Rax), r_op));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::ShiftLeft => {
                if let X86Operand::Imm(shift) = r_op {
                    if matches!(d_op, X86Operand::Reg(_)) {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Shl(d_op, X86Operand::Imm(shift)));
                    } else {
                        asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        asm.push(X86Instr::Shl(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(shift)));
                        asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    asm.push(X86Instr::Shl(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            BinaryOp::ShiftRight => {
                if let X86Operand::Imm(shift) = r_op {
                    if matches!(d_op, X86Operand::Reg(_)) {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Shr(d_op, X86Operand::Imm(shift)));
                    } else {
                        asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                        asm.push(X86Instr::Shr(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(shift)));
                        asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                    }
                } else {
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), l_op));
                    asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), r_op));
                    asm.push(X86Instr::Shr(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
                    asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                }
            }
            _ => {}
        }
    }

    pub fn gen_unary_op(
        asm: &mut Vec<X86Instr>,
        _dest: VarId,
        op: &UnaryOp,
        s_op: X86Operand,
        d_op: X86Operand,
    ) {
        match op {
            UnaryOp::Minus => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                asm.push(X86Instr::Sub(X86Operand::Reg(X86Reg::Rax), s_op));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::LogicalNot => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Imm(0)));
                asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::BitwiseNot => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                asm.push(X86Instr::Not(X86Operand::Reg(X86Reg::Rax)));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::Plus => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), s_op));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            }
            UnaryOp::AddrOf | UnaryOp::Deref => unreachable!("AddrOf and Deref should be lowered by IR"),
        }
    }

    #[allow(dead_code)]
    pub fn gen_float_binary_op(
        asm: &mut Vec<X86Instr>,
        var_types: &mut HashMap<VarId, Type>,
        dest: VarId,
        op: &BinaryOp,
        left: &Operand,
        right: &Operand,
        d_op: X86Operand,
        mut get_operand_to_op: impl FnMut(&Operand) -> X86Operand,
        mut get_var_to_op: impl FnMut(VarId) -> X86Operand,
    ) {
        // Record that result is a float
        var_types.insert(dest, Type::Float);
        
        // Load left operand into xmm0
        match left {
            Operand::FloatConstant(_) => {
                let left_label = get_operand_to_op(left);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_label));
            }
            Operand::Var(v) => {
                let left_op = get_var_to_op(*v);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_op));
            }
            Operand::Constant(c) => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        // Load right operand into xmm1
        match right {
            Operand::FloatConstant(_) => {
                let right_label = get_operand_to_op(right);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_label));
            }
            Operand::Var(v) => {
                let right_op = get_var_to_op(*v);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_op));
            }
            Operand::Constant(c) => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        // Perform operation
        match op {
            BinaryOp::Add => {
                asm.push(X86Instr::Addss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Sub => {
                asm.push(X86Instr::Subss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Mul => {
                asm.push(X86Instr::Mulss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Div => {
                asm.push(X86Instr::Divss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::EqualEqual | BinaryOp::NotEqual => {
                asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                let cond = match op {
                    BinaryOp::Less => "b",
                    BinaryOp::LessEqual => "be",
                    BinaryOp::Greater => "a",
                    BinaryOp::GreaterEqual => "ae",
                    BinaryOp::EqualEqual => "e",
                    BinaryOp::NotEqual => "ne",
                    _ => unreachable!(),
                };
                asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                return;
            }
            _ => {
                asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
            }
        }
        
        asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    }

    #[allow(dead_code)]
    pub fn gen_float_unary_op(
        asm: &mut Vec<X86Instr>,
        var_types: &mut HashMap<VarId, Type>,
        dest: VarId,
        op: &UnaryOp,
        src: &Operand,
        d_op: X86Operand,
        mut get_operand_to_op: impl FnMut(&Operand) -> X86Operand,
        mut get_var_to_op: impl FnMut(VarId) -> X86Operand,
        mut get_or_create_float_const: impl FnMut(f64) -> String,
    ) {
        var_types.insert(dest, Type::Float);
        
        // Load source into xmm0
        match src {
            Operand::FloatConstant(_) => {
                let src_label = get_operand_to_op(src);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_label));
            }
            Operand::Var(v) => {
                let src_op = get_var_to_op(*v);
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_op));
            }
            Operand::Constant(c) => {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
                asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
            }
            _ => {}
        }
        
        match op {
            UnaryOp::Minus => {
                let sign_bit_label = get_or_create_float_const(f64::from_bits(0x8000000000000000u64));
                asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::RipRelLabel(sign_bit_label)));
                asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            }
            UnaryOp::LogicalNot => {
                asm.push(X86Instr::Cvttss2si(X86Operand::Reg(X86Reg::Eax), X86Operand::Reg(X86Reg::Xmm0)));
                asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(0)));
                asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
                return;
            }
            _ => {
                asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
            }
        }
        
        asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    }
}
