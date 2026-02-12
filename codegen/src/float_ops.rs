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
            generator.asm.push(X86Instr::Ucomiss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            let cond = match op {
                BinaryOp::Less => "b",
                BinaryOp::LessEqual => "be",
                BinaryOp::Greater => "a",
                BinaryOp::GreaterEqual => "ae",
                BinaryOp::EqualEqual => "e",
                BinaryOp::NotEqual => "ne",
                _ => unreachable!(),
            };
            generator.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
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
            generator.asm.push(X86Instr::Cvttss2si(X86Operand::Reg(X86Reg::Eax), X86Operand::Reg(X86Reg::Xmm0)));
            generator.asm.push(X86Instr::Cmp(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(0)));
            generator.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            return;
        }
        _ => {}
    }
    generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
}
