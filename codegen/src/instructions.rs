use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::{BinaryOp, UnaryOp};
use ir::VarId;

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
        let is_32bit_op = |op: &X86Operand| -> bool {
            match op {
                X86Operand::DwordMem(..) | X86Operand::FloatMem(..) => true,
                X86Operand::Reg(r) => matches!(r, X86Reg::Eax | X86Reg::Ecx | X86Reg::Edx | X86Reg::Ebx | X86Reg::Esi | X86Reg::Edi | X86Reg::Esp | X86Reg::Ebp | X86Reg::R8d | X86Reg::R9d | X86Reg::R10d | X86Reg::R11d | X86Reg::R12d | X86Reg::R13d | X86Reg::R14d | X86Reg::R15d),
                _ => false
            }
        };

        let op_is_32bit = is_32bit_op(&l_op) || is_32bit_op(&r_op) || is_32bit_op(&d_op);
        let cmp_is_32bit = is_32bit_op(&l_op) || is_32bit_op(&r_op);
        let dest_is_32bit = is_32bit_op(&d_op);

        let get_regs = |is_32| if is_32 { 
            (X86Reg::Eax, X86Reg::Ecx, X86Reg::Edx) 
        } else { 
            (X86Reg::Rax, X86Reg::Rcx, X86Reg::Rdx) 
        };

        let (ax, cx, dx) = get_regs(op_is_32bit);
        let ax_op = X86Operand::Reg(ax.clone());

        match op {
            BinaryOp::Add => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest + right -> just add
                        asm.push(X86Instr::Add(d_op, r_op));
                    } else if d_op == r_op {
                         asm.push(X86Instr::Add(d_op, l_op));
                    } else {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Add(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Add(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::Sub => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest - right -> just sub
                        asm.push(X86Instr::Sub(d_op, r_op));
                    } else if d_op == r_op {
                         asm.push(X86Instr::Neg(d_op.clone()));
                         asm.push(X86Instr::Add(d_op, l_op));
                    } else {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Sub(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Sub(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::Mul => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest * right -> just imul
                        asm.push(X86Instr::Imul(d_op, r_op));
                    } else if d_op == r_op {
                        asm.push(X86Instr::Imul(d_op, l_op));
                    } else {
                         asm.push(X86Instr::Mov(d_op.clone(), l_op));
                         asm.push(X86Instr::Imul(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Imul(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::Div => {
                asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                if op_is_32bit { asm.push(X86Instr::Cdq); } else { asm.push(X86Instr::Cqto); }
                
                let div_op = if let X86Operand::Imm(_) = r_op {
                    asm.push(X86Instr::Mov(X86Operand::Reg(cx.clone()), r_op));
                    X86Operand::Reg(cx)
                } else {
                    r_op
                };
                asm.push(X86Instr::Idiv(div_op));
                asm.push(X86Instr::Mov(d_op, ax_op));
            }
            BinaryOp::Mod => {
                asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                if op_is_32bit { asm.push(X86Instr::Cdq); } else { asm.push(X86Instr::Cqto); }
                
                let div_op = if let X86Operand::Imm(_) = r_op {
                    asm.push(X86Instr::Mov(X86Operand::Reg(cx.clone()), r_op));
                    X86Operand::Reg(cx)
                } else {
                    r_op
                };
                asm.push(X86Instr::Idiv(div_op));
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(dx)));
            }
            BinaryOp::EqualEqual | BinaryOp::NotEqual | BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                let (mut c_ax, c_cx, _) = get_regs(cmp_is_32bit);
                
                // If r_op uses the scratch register (EAX/RAX), use ECX/RCX instead
                if let X86Operand::Reg(r) = &r_op {
                    if *r == c_ax {
                        c_ax = c_cx;
                    }
                }

                asm.push(X86Instr::Mov(X86Operand::Reg(c_ax.clone()), l_op));
                asm.push(X86Instr::Cmp(X86Operand::Reg(c_ax), r_op));
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
                let (d_ax, _, _) = get_regs(dest_is_32bit);
                asm.push(X86Instr::Mov(d_op, X86Operand::Reg(d_ax)));
            }
            BinaryOp::BitwiseAnd => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest & right -> just and
                        asm.push(X86Instr::And(d_op, r_op));
                    } else if d_op == r_op {
                        // Optimize: dest = left & dest -> just and (commutative)
                        asm.push(X86Instr::And(d_op, l_op));
                    } else {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::And(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::And(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::BitwiseOr => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest | right -> just or
                        asm.push(X86Instr::Or(d_op, r_op));
                    } else if d_op == r_op {
                        // Optimize: dest = left | dest -> just or (commutative)
                        asm.push(X86Instr::Or(d_op, l_op));
                    } else {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Or(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Or(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::BitwiseXor => {
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
                        // Optimize: dest = dest ^ right -> just xor
                        asm.push(X86Instr::Xor(d_op, r_op));
                    } else if d_op == r_op {
                        // Optimize: dest = left ^ dest -> just xor (commutative)
                        asm.push(X86Instr::Xor(d_op, l_op));
                    } else {
                        asm.push(X86Instr::Mov(d_op.clone(), l_op));
                        asm.push(X86Instr::Xor(d_op, r_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Xor(ax_op.clone(), r_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::ShiftLeft => {
                let count_op = if let X86Operand::Imm(_) = r_op {
                    r_op
                } else {
                    let (_, c_cx, _) = get_regs(is_32bit_op(&r_op));
                    asm.push(X86Instr::Mov(X86Operand::Reg(c_cx), r_op));
                    X86Operand::Reg(X86Reg::Rcx)
                };
                
                if matches!(d_op, X86Operand::Reg(_)) && d_op == l_op {
                    // Optimize: dest = dest << count -> just shl
                    asm.push(X86Instr::Shl(d_op, count_op));
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Shl(ax_op.clone(), count_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            BinaryOp::ShiftRight => {
                let count_op = if let X86Operand::Imm(_) = r_op {
                    r_op
                } else {
                    let (_, c_cx, _) = get_regs(is_32bit_op(&r_op));
                    asm.push(X86Instr::Mov(X86Operand::Reg(c_cx), r_op));
                    X86Operand::Reg(X86Reg::Rcx)
                };
                
                if matches!(d_op, X86Operand::Reg(_)) && d_op == l_op {
                    // Optimize: dest = dest >> count -> just shr
                    asm.push(X86Instr::Shr(d_op, count_op));
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    asm.push(X86Instr::Shr(ax_op.clone(), count_op));
                    asm.push(X86Instr::Mov(d_op, ax_op));
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
        let is_32bit = match &d_op {
            X86Operand::DwordMem(..) | X86Operand::FloatMem(..) => true,
            X86Operand::Reg(r) => matches!(r, X86Reg::Eax | X86Reg::Ecx | X86Reg::Edx | X86Reg::Ebx | X86Reg::Esi | X86Reg::Edi | X86Reg::Esp | X86Reg::Ebp | X86Reg::R8d | X86Reg::R9d | X86Reg::R10d | X86Reg::R11d | X86Reg::R12d | X86Reg::R13d | X86Reg::R14d | X86Reg::R15d),
            _ => false
        };
        
        let ax = if is_32bit { X86Reg::Eax } else { X86Reg::Rax };
        let ax_op = X86Operand::Reg(ax.clone());

        match op {
            UnaryOp::Minus => {
                if matches!(d_op, X86Operand::Reg(_)) && d_op == s_op {
                    // Optimize: dest = -dest -> just neg
                    asm.push(X86Instr::Neg(d_op));
                } else if matches!(d_op, X86Operand::Reg(_)) {
                    // Optimize: if dest is a register, can negate directly
                    asm.push(X86Instr::Mov(d_op.clone(), s_op));
                    asm.push(X86Instr::Neg(d_op));
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), s_op));
                    asm.push(X86Instr::Neg(ax_op.clone()));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            UnaryOp::LogicalNot => {
                // Determine source size for CMP
                let src_is_32bit = match &s_op {
                    X86Operand::DwordMem(..) | X86Operand::FloatMem(..) => true,
                    X86Operand::Reg(r) => matches!(r, X86Reg::Eax | X86Reg::Ecx | X86Reg::Edx | X86Reg::Ebx | X86Reg::Esi | X86Reg::Edi | X86Reg::Esp | X86Reg::Ebp | X86Reg::R8d | X86Reg::R9d | X86Reg::R10d | X86Reg::R11d | X86Reg::R12d | X86Reg::R13d | X86Reg::R14d | X86Reg::R15d),
                    _ => false
                };
                let s_ax = if src_is_32bit { X86Reg::Eax } else { X86Reg::Rax };
                
                asm.push(X86Instr::Mov(X86Operand::Reg(s_ax.clone()), s_op));
                asm.push(X86Instr::Cmp(X86Operand::Reg(s_ax), X86Operand::Imm(0)));
                asm.push(X86Instr::Mov(ax_op.clone(), X86Operand::Imm(0)));
                asm.push(X86Instr::Set("e".to_string(), X86Operand::Reg(X86Reg::Al)));
                asm.push(X86Instr::Mov(d_op, ax_op));
            }
            UnaryOp::BitwiseNot => {
                if matches!(d_op, X86Operand::Reg(_)) && d_op == s_op {
                    // Optimize: dest = ~dest -> just not
                    asm.push(X86Instr::Not(d_op));
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), s_op));
                    asm.push(X86Instr::Not(ax_op.clone()));
                    asm.push(X86Instr::Mov(d_op, ax_op));
                }
            }
            UnaryOp::Plus => {
                // Unary plus is identity: just move source to destination
                if d_op != s_op {
                    asm.push(X86Instr::Mov(d_op, s_op));
                }
                // If d_op == s_op, no operation needed at all
            }
            UnaryOp::AddrOf | UnaryOp::Deref => unreachable!("AddrOf and Deref should be lowered by IR"),
        }
    }

}
