use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::{BinaryOp, UnaryOp, Type};
use ir::{VarId, Operand};

pub fn gen_float_binary_op(generator: &mut FunctionGenerator, dest: VarId, op: &BinaryOp, left: &Operand, right: &Operand) {
    // Load left operand into xmm0
    match left {
        Operand::FloatConstant(_) => {
            let left_label = generator.operand_to_op(left);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_label));
        }
        Operand::Var(v) => {
            let left_op = generator.var_to_op(*v);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), left_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    
    // Load right operand into xmm1
    match right {
        Operand::FloatConstant(_) => {
            let right_label = generator.operand_to_op(right);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_label));
        }
        Operand::Var(v) => {
            let right_op = generator.var_to_op(*v);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), right_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    
    // Perform operation
    match op {
        BinaryOp::Add => {
            generator.var_types.insert(dest, Type::Float);
            generator.asm.push(X86Instr::Addss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Sub => {
            generator.var_types.insert(dest, Type::Float);
            generator.asm.push(X86Instr::Subss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Mul => {
            generator.var_types.insert(dest, Type::Float);
            generator.asm.push(X86Instr::Mulss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Div => {
            generator.var_types.insert(dest, Type::Float);
            generator.asm.push(X86Instr::Divss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::EqualEqual | BinaryOp::NotEqual => {
            generator.var_types.insert(dest, Type::Int);
            // IEEE 754 NaN handling: ucomiss sets PF=1 for unordered (NaN) operands.
            // - For Less/LessEqual: swap operands and use seta/setae (CF=0 for NaN → false)
            // - For Greater/GreaterEqual: seta/setae already correct (CF=1 for NaN → false)
            // - For EqualEqual: sete AND setnp (must be equal AND ordered)
            // - For NotEqual: setne OR setp (not-equal OR unordered)
            match op {
                BinaryOp::Less | BinaryOp::LessEqual => {
                    // Swap operands: a < b ≡ b above a
                    generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Xmm0)));
                    let cond = if *op == BinaryOp::Less { "a" } else { "ae" };
                    generator.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                }
                BinaryOp::Greater | BinaryOp::GreaterEqual => {
                    generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                    let cond = if *op == BinaryOp::Greater { "a" } else { "ae" };
                    generator.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                }
                BinaryOp::EqualEqual => {
                    generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                    generator.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                    generator.asm.push(X86Instr::Set("np".to_string(), X86Operand::Reg(X86Reg::Cl)));
                    generator.asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Al), X86Operand::Reg(X86Reg::Cl)));
                }
                BinaryOp::NotEqual => {
                    generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                    generator.asm.push(X86Instr::Set("ne".to_string(), X86Operand::Reg(X86Reg::Al)));
                    generator.asm.push(X86Instr::Set("p".to_string(), X86Operand::Reg(X86Reg::Cl)));
                    generator.asm.push(X86Instr::Or(X86Operand::Reg(X86Reg::Al), X86Operand::Reg(X86Reg::Cl)));
                }
                _ => unreachable!(),
            }
            let dest_op = generator.var_to_op(dest); 
            generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
            return; 
        }
        _ => {
            generator.var_types.insert(dest, Type::Float);
            generator.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
        }
    }
    let dest_op = generator.var_to_op(dest);
    generator.asm.push(X86Instr::Movss(dest_op, X86Operand::Reg(X86Reg::Xmm0)));
}

pub fn gen_float_unary_op(generator: &mut FunctionGenerator, dest: VarId, op: &UnaryOp, src: &Operand) {
    generator.var_types.insert(dest, Type::Float);
    let d_op = generator.var_to_op(dest);
    match src {
        Operand::FloatConstant(_) => {
            let src_label = generator.operand_to_op(src);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_label));
        }
        Operand::Var(v) => {
            let src_op = generator.var_to_op(*v);
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), src_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(X86Instr::Cvtsi2ss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    match op {
        UnaryOp::Minus => {
            let sign_bit_label = generator.get_or_create_float_const(f64::from_bits(0x8000000000000000u64));
            generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm1), X86Operand::RipRelLabel(sign_bit_label)));
            generator.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        UnaryOp::LogicalNot => {
            // !float: compare against 0.0, must be ordered AND equal
            // NaN → unordered → setnp=0 → result=0 (correct: !NaN = false)
            // denormals → not equal to 0.0 → sete=0 → result=0 (correct)
            generator.asm.push(X86Instr::Xorps(X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Xmm1)));
            generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            generator.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Set("np".to_string(), X86Operand::Reg(X86Reg::Cl)));
            generator.asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Al), X86Operand::Reg(X86Reg::Cl)));
            generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            return;
        }
        _ => {}
    }
    generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
}
