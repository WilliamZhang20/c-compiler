use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::{BinaryOp, UnaryOp, Type};
use ir::{VarId, Operand};

/// Helper: pick the right mov variant based on float vs double.
fn movfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Movsd(d, s) } else { X86Instr::Movss(d, s) }
}
fn addfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Addsd(d, s) } else { X86Instr::Addss(d, s) }
}
fn subfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Subsd(d, s) } else { X86Instr::Subss(d, s) }
}
fn mulfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Mulsd(d, s) } else { X86Instr::Mulss(d, s) }
}
fn divfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Divsd(d, s) } else { X86Instr::Divss(d, s) }
}
fn ucomifp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Ucomisd(d, s) } else { X86Instr::Ucomiss(d, s) }
}
fn cvtsi2fp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Cvtsi2sd(d, s) } else { X86Instr::Cvtsi2ss(d, s) }
}
fn xorpfp(is_double: bool, d: X86Operand, s: X86Operand) -> X86Instr {
    if is_double { X86Instr::Xorpd(d, s) } else { X86Instr::Xorps(d, s) }
}

/// Determine if a float binary operation should produce double-precision output.
fn infer_double(generator: &FunctionGenerator, left: &Operand, right: &Operand) -> bool {
    let left_double = match left {
        Operand::Var(v) => generator.var_types.get(v).map(|t| matches!(t, Type::Double)).unwrap_or(false),
        _ => false,
    };
    let right_double = match right {
        Operand::Var(v) => generator.var_types.get(v).map(|t| matches!(t, Type::Double)).unwrap_or(false),
        _ => false,
    };
    left_double || right_double
}

pub fn gen_float_binary_op(generator: &mut FunctionGenerator, dest: VarId, op: &BinaryOp, left: &Operand, right: &Operand) {
    let is_double = infer_double(generator, left, right);
    let result_type = if is_double { Type::Double } else { Type::Float };

    // Load left operand into xmm0
    match left {
        Operand::FloatConstant(f) => {
            let label = generator.get_or_create_float_const(*f, is_double);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::RipRelLabel(label)));
        }
        Operand::Var(v) => {
            let left_op = generator.var_to_op(*v);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm0), left_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(cvtsi2fp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    
    // Load right operand into xmm1
    match right {
        Operand::FloatConstant(f) => {
            let label = generator.get_or_create_float_const(*f, is_double);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm1), X86Operand::RipRelLabel(label)));
        }
        Operand::Var(v) => {
            let right_op = generator.var_to_op(*v);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm1), right_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(cvtsi2fp(is_double, X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    
    // Perform operation
    match op {
        BinaryOp::Add => {
            generator.var_types.insert(dest, result_type);
            generator.asm.push(addfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Sub => {
            generator.var_types.insert(dest, result_type);
            generator.asm.push(subfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Mul => {
            generator.var_types.insert(dest, result_type);
            generator.asm.push(mulfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Div => {
            generator.var_types.insert(dest, result_type);
            generator.asm.push(divfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual | BinaryOp::EqualEqual | BinaryOp::NotEqual => {
            generator.var_types.insert(dest, Type::Int);
            match op {
                BinaryOp::Less | BinaryOp::LessEqual => {
                    generator.asm.push(ucomifp(is_double, X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Xmm0)));
                    let cond = if *op == BinaryOp::Less { "a" } else { "ae" };
                    generator.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                }
                BinaryOp::Greater | BinaryOp::GreaterEqual => {
                    generator.asm.push(ucomifp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                    let cond = if *op == BinaryOp::Greater { "a" } else { "ae" };
                    generator.asm.push(X86Instr::Set(cond.to_string(), X86Operand::Reg(X86Reg::Al)));
                }
                BinaryOp::EqualEqual => {
                    generator.asm.push(ucomifp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
                    generator.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                    generator.asm.push(X86Instr::Set("np".to_string(), X86Operand::Reg(X86Reg::Cl)));
                    generator.asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Al), X86Operand::Reg(X86Reg::Cl)));
                }
                BinaryOp::NotEqual => {
                    generator.asm.push(ucomifp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
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
            generator.var_types.insert(dest, result_type);
            generator.asm.push(xorpfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm0)));
        }
    }
    let dest_op = generator.var_to_op(dest);
    generator.asm.push(movfp(is_double, dest_op, X86Operand::Reg(X86Reg::Xmm0)));
}

pub fn gen_float_unary_op(generator: &mut FunctionGenerator, dest: VarId, op: &UnaryOp, src: &Operand) {
    let is_double = match src {
        Operand::Var(v) => generator.var_types.get(v).map(|t| matches!(t, Type::Double)).unwrap_or(false),
        _ => false,
    };
    let result_type = if is_double { Type::Double } else { Type::Float };
    generator.var_types.insert(dest, result_type);
    let d_op = generator.var_to_op(dest);
    match src {
        Operand::FloatConstant(f) => {
            let label = generator.get_or_create_float_const(*f, is_double);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::RipRelLabel(label)));
        }
        Operand::Var(v) => {
            let src_op = generator.var_to_op(*v);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm0), src_op));
        }
        Operand::Constant(c) => {
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::Imm(*c)));
            generator.asm.push(cvtsi2fp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Eax)));
        }
        _ => {}
    }
    match op {
        UnaryOp::Minus => {
            // -0.0 has the sign bit set; works for both float (.long) and double (.quad)
            let sign_mask = f64::from_bits(0x8000000000000000u64);
            let sign_bit_label = generator.get_or_create_float_const(sign_mask, is_double);
            generator.asm.push(movfp(is_double, X86Operand::Reg(X86Reg::Xmm1), X86Operand::RipRelLabel(sign_bit_label)));
            generator.asm.push(xorpfp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
        }
        UnaryOp::LogicalNot => {
            generator.asm.push(xorpfp(is_double, X86Operand::Reg(X86Reg::Xmm1), X86Operand::Reg(X86Reg::Xmm1)));
            generator.asm.push(ucomifp(is_double, X86Operand::Reg(X86Reg::Xmm0), X86Operand::Reg(X86Reg::Xmm1)));
            generator.asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Set("np".to_string(), X86Operand::Reg(X86Reg::Cl)));
            generator.asm.push(X86Instr::And(X86Operand::Reg(X86Reg::Al), X86Operand::Reg(X86Reg::Cl)));
            generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Al)));
            generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            return;
        }
        _ => {}
    }
    generator.asm.push(movfp(is_double, d_op, X86Operand::Reg(X86Reg::Xmm0)));
}
