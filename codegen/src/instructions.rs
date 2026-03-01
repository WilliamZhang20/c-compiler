use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::{BinaryOp, UnaryOp};
use ir::VarId;

/// Instruction generation for arithmetic and logical operations
pub struct InstructionGenerator;

/// Magic number for signed division by constant (Hacker's Delight algorithm).
/// Returns (magic_number, shift) such that:
///   x / d ≈ MULHI(x, magic) >> shift  (with correction for sign)
fn signed_div_magic_64(d: i64) -> Option<(i64, u32)> {
    if d <= 0 || d == 1 { return None; }
    
    let ad = d as u64;
    let t = (1u64 << 63) + ((d >> 63) as u64);  // 2^63 + (sign bit)
    let anc = t - 1 - (t % ad);  // abs(nc)
    
    let mut p = 63u32;
    let mut q1 = (1u64 << 63) / anc;
    let mut r1 = (1u64 << 63) - q1 * anc;
    let mut q2 = (1u64 << 63) / ad;
    let mut r2 = (1u64 << 63) - q2 * ad;
    
    loop {
        p += 1;
        if p > 128 { return None; }
        
        q1 = q1.wrapping_mul(2);
        r1 = r1.wrapping_mul(2);
        if r1 >= anc { q1 += 1; r1 -= anc; }
        
        q2 = q2.wrapping_mul(2);
        r2 = r2.wrapping_mul(2);
        if r2 >= ad { q2 += 1; r2 -= ad; }
        
        let delta = ad - r2;
        if q1 < delta || (q1 == delta && r1 == 0) {
            continue;
        }
        break;
    }
    
    let magic = (q2 + 1) as i64;
    let shift = p - 64;
    Some((magic, shift))
}

/// Emit optimized signed division by constant for 64-bit values.
/// Returns true if optimization was applied.
fn emit_div_by_const_64(
    asm: &mut Vec<X86Instr>,
    l_op: X86Operand,
    d: i64,
    d_op: X86Operand,
    want_remainder: bool,
) -> bool {
    if d <= 0 { return false; }
    
    // Power of 2: use shifts
    if d > 0 && (d as u64).is_power_of_two() {
        let shift = d.trailing_zeros();
        if shift == 0 {
            // Division by 1: just move
            asm.push(X86Instr::Mov(d_op, l_op));
            return true;
        }
        // Signed division by power of 2:
        // q = (x + ((x >> 63) >>> (64 - shift))) >> shift
        let rax = X86Operand::Reg(X86Reg::Rax);
        let rdx = X86Operand::Reg(X86Reg::Rdx);
        asm.push(X86Instr::Mov(rax.clone(), l_op.clone()));
        asm.push(X86Instr::Mov(rdx.clone(), rax.clone()));
        asm.push(X86Instr::Sar(rdx.clone(), X86Operand::Imm(63)));
        asm.push(X86Instr::Shr(rdx.clone(), X86Operand::Imm(64 - shift as i64)));
        asm.push(X86Instr::Add(rax.clone(), rdx.clone()));
        if want_remainder {
            // r = x - (q << shift) * 1  =>  r = x - (q >> shift) << shift ... 
            // Easier: r = x & (d-1), adjusted for sign
            // Actually: remainder = x - quotient * d
            asm.push(X86Instr::Sar(rax.clone(), X86Operand::Imm(shift as i64)));
            // rax = quotient; now compute remainder = l_op - quotient * d
            asm.push(X86Instr::Imul(rax.clone(), X86Operand::Imm(d)));
            // rdx = l_op
            asm.push(X86Instr::Mov(rdx.clone(), l_op));
            asm.push(X86Instr::Sub(rdx.clone(), rax.clone()));
            asm.push(X86Instr::Mov(d_op, rdx));
        } else {
            asm.push(X86Instr::Sar(rax.clone(), X86Operand::Imm(shift as i64)));
            asm.push(X86Instr::Mov(d_op, rax));
        }
        return true;
    }
    
    // General case: magic number multiplication
    if let Some((magic, shift)) = signed_div_magic_64(d) {
        let rax = X86Operand::Reg(X86Reg::Rax);
        let rdx = X86Operand::Reg(X86Reg::Rdx);
        let rcx = X86Operand::Reg(X86Reg::Rcx);
        
        // Save dividend in rcx for remainder calculation
        if want_remainder {
            asm.push(X86Instr::Mov(rcx.clone(), l_op.clone()));
        }
        
        // Load magic number into rax
        asm.push(X86Instr::Raw(format!("mov rax, {}", magic)));
        // imul rdx:rax, l_op  (signed multiply, high result in rdx)
        asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R11), l_op.clone()));
        asm.push(X86Instr::Raw("imul r11".to_string()));
        // rdx now has the high 64 bits
        
        // If magic is negative, add the dividend
        if magic < 0 {
            if want_remainder {
                asm.push(X86Instr::Add(rdx.clone(), rcx.clone()));
            } else {
                asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R11), l_op.clone()));
                asm.push(X86Instr::Add(rdx.clone(), X86Operand::Reg(X86Reg::R11)));
            }
        }
        
        // Arithmetic shift right by 'shift'
        if shift > 0 {
            asm.push(X86Instr::Sar(rdx.clone(), X86Operand::Imm(shift as i64)));
        }
        
        // Add sign bit correction: q += (q >> 63) i.e. add 1 if negative
        asm.push(X86Instr::Mov(rax.clone(), rdx.clone()));
        asm.push(X86Instr::Shr(rax.clone(), X86Operand::Imm(63)));
        asm.push(X86Instr::Add(rdx.clone(), rax.clone()));
        // rdx = quotient
        
        if want_remainder {
            // remainder = dividend - quotient * d
            asm.push(X86Instr::Mov(rax.clone(), rdx.clone()));
            asm.push(X86Instr::Imul(rax.clone(), X86Operand::Imm(d)));
            asm.push(X86Instr::Sub(rcx.clone(), rax.clone()));
            asm.push(X86Instr::Mov(d_op, rcx));
        } else {
            asm.push(X86Instr::Mov(d_op, rdx));
        }
        return true;
    }
    
    false
}

/// Emit optimized multiply by constant using LEA/shift sequences.
/// Returns true if optimization was applied.
fn emit_mul_by_const(asm: &mut Vec<X86Instr>, src: &X86Reg, dest: &X86Reg, c: i64) -> bool {
    let s = src.to_str();
    let dt = dest.to_str();
    
    match c {
        0 => {
            asm.push(X86Instr::Xor(X86Operand::Reg(dest.clone()), X86Operand::Reg(dest.clone())));
            true
        }
        1 => {
            if !src.same_physical(dest) {
                asm.push(X86Instr::Mov(X86Operand::Reg(dest.clone()), X86Operand::Reg(src.clone())));
            }
            true
        }
        -1 => {
            if !src.same_physical(dest) {
                asm.push(X86Instr::Mov(X86Operand::Reg(dest.clone()), X86Operand::Reg(src.clone())));
            }
            asm.push(X86Instr::Neg(X86Operand::Reg(dest.clone())));
            true
        }
        c if c > 0 && (c as u64).is_power_of_two() => {
            let shift = (c as u64).trailing_zeros();
            if !src.same_physical(dest) {
                asm.push(X86Instr::Mov(X86Operand::Reg(dest.clone()), X86Operand::Reg(src.clone())));
            }
            asm.push(X86Instr::Shl(X86Operand::Reg(dest.clone()), X86Operand::Imm(shift as i64)));
            true
        }
        3 => {
            // lea dest, [src + src*2]
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*2]", dt, s, s)));
            true
        }
        5 => {
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*4]", dt, s, s)));
            true
        }
        9 => {
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*8]", dt, s, s)));
            true
        }
        6 => {
            // x*6 = (x + x*2) * 2
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*2]", dt, s, s)));
            let d2 = X86Operand::Reg(dest.clone());
            asm.push(X86Instr::Add(d2.clone(), d2));
            true
        }
        7 => {
            // x*7 = x*8 - x
            asm.push(X86Instr::Raw(format!("lea {}, [{}*8]", dt, s)));
            asm.push(X86Instr::Sub(X86Operand::Reg(dest.clone()), X86Operand::Reg(src.clone())));
            true
        }
        10 => {
            // x*10 = (x + x*4) * 2
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*4]", dt, s, s)));
            let d2 = X86Operand::Reg(dest.clone());
            asm.push(X86Instr::Add(d2.clone(), d2));
            true
        }
        11 => {
            // x*11 = x + (x + x*4)*2
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*4]", dt, s, s)));
            let d2 = X86Operand::Reg(dest.clone());
            asm.push(X86Instr::Add(d2.clone(), d2));
            asm.push(X86Instr::Add(X86Operand::Reg(dest.clone()), X86Operand::Reg(src.clone())));
            true
        }
        12 => {
            // x*12 = (x + x*2) * 4 = lea + shl 2
            asm.push(X86Instr::Raw(format!("lea {}, [{} + {}*2]", dt, s, s)));
            asm.push(X86Instr::Shl(X86Operand::Reg(dest.clone()), X86Operand::Imm(2)));
            true
        }
        _ => false,
    }
}

impl InstructionGenerator {
    pub fn gen_binary_op(
        asm: &mut Vec<X86Instr>,
        _dest: VarId,
        op: &BinaryOp,
        l_op: X86Operand,
        r_op: X86Operand,
        d_op: X86Operand,
        is_signed: bool,
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
                    } else if let (X86Operand::Reg(d_reg), X86Operand::Reg(l_reg), X86Operand::Imm(imm)) = (&d_op, &l_op, &r_op) {
                        // dest = left + imm -> lea dest, [left + imm]
                        if *imm >= -2147483648 && *imm <= 2147483647 {
                            asm.push(X86Instr::Lea(X86Operand::Reg(d_reg.clone()), X86Operand::Mem(l_reg.clone(), *imm as i32)));
                        } else {
                            asm.push(X86Instr::Mov(d_op.clone(), l_op));
                            asm.push(X86Instr::Add(d_op, r_op));
                        }
                    } else if let (X86Operand::Reg(d_reg), X86Operand::Reg(l_reg), X86Operand::Reg(r_reg)) = (&d_op, &l_op, &r_op) {
                        // dest = left + right -> lea dest, [left + right]
                        asm.push(X86Instr::Raw(format!("lea {}, [{} + {}]", d_reg.to_str(), l_reg.to_str(), r_reg.to_str())));
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
                    } else if let (X86Operand::Reg(d_reg), X86Operand::Reg(l_reg), X86Operand::Imm(imm)) = (&d_op, &l_op, &r_op) {
                        // dest = left - imm -> lea dest, [left - imm]
                        let neg_imm = -(*imm);
                        if neg_imm >= -2147483648 && neg_imm <= 2147483647 {
                            asm.push(X86Instr::Lea(X86Operand::Reg(d_reg.clone()), X86Operand::Mem(l_reg.clone(), neg_imm as i32)));
                        } else {
                            asm.push(X86Instr::Mov(d_op.clone(), l_op));
                            asm.push(X86Instr::Sub(d_op, r_op));
                        }
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
                // Try strength reduction for multiply by small constants
                if let X86Operand::Imm(c) = &r_op {
                    if let X86Operand::Reg(d_reg) = &d_op {
                        if let X86Operand::Reg(l_reg) = &l_op {
                            let emitted = emit_mul_by_const(asm, l_reg, d_reg, *c);
                            if emitted { return; }
                        }
                    }
                }
                // Symmetric: try swapping left/right for constant on left
                if let X86Operand::Imm(c) = &l_op {
                    if let X86Operand::Reg(d_reg) = &d_op {
                        if let X86Operand::Reg(r_reg) = &r_op {
                            let emitted = emit_mul_by_const(asm, r_reg, d_reg, *c);
                            if emitted { return; }
                        }
                    }
                }
                if matches!(d_op, X86Operand::Reg(_)) {
                    if d_op == l_op {
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
                // Try strength reduction for constant divisor
                if let X86Operand::Imm(d) = &r_op {
                    if !op_is_32bit && emit_div_by_const_64(asm, l_op.clone(), *d, d_op.clone(), false) {
                        return;
                    }
                }
                // Fallback to idiv
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
                // Try strength reduction for constant divisor
                if let X86Operand::Imm(d) = &r_op {
                    if !op_is_32bit && emit_div_by_const_64(asm, l_op.clone(), *d, d_op.clone(), true) {
                        return;
                    }
                }
                // Fallback to idiv
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
                    // Optimize: dest = dest >> count -> just shr/sar
                    if is_signed {
                        asm.push(X86Instr::Sar(d_op, count_op));
                    } else {
                        asm.push(X86Instr::Shr(d_op, count_op));
                    }
                } else {
                    asm.push(X86Instr::Mov(ax_op.clone(), l_op));
                    if is_signed {
                        asm.push(X86Instr::Sar(ax_op.clone(), count_op));
                    } else {
                        asm.push(X86Instr::Shr(ax_op.clone(), count_op));
                    }
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
