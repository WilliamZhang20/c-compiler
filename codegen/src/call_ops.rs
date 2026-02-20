use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use crate::calling_convention::get_convention;
use model::Type;
use ir::{VarId, Operand};

/// A pending assignment to a param register.
enum ParamMove {
    /// emit: lea param_reg, operand
    Lea(X86Operand),
    /// emit: mov param_reg, operand
    Mov(X86Operand),
}

impl ParamMove {
    fn src_operand(&self) -> &X86Operand {
        match self {
            ParamMove::Lea(op) | ParamMove::Mov(op) => op,
        }
    }
}

/// Emit all integer param-register assignments using a cycle-safe parallel-move algorithm.
/// `moves` is a list of (param_reg_index, ParamMove) pairs.
/// rax is used as a scratch register to break cycles.
fn emit_parallel_int_moves(generator: &mut FunctionGenerator,
                           param_regs: &[X86Reg],
                           mut moves: Vec<(usize, ParamMove)>) {
    loop {
        if moves.is_empty() { break; }

        // Find an entry whose destination param_reg is NOT read as a Mov source by any other entry.
        // Lea sources are RipRelLabel/Mem and can never equal a param reg, so they're always safe.
        let safe_pos = moves.iter().position(|&(di, _)| {
            let dst = X86Operand::Reg(param_regs[di].clone());
            !moves.iter().any(|(_, pm)| {
                if let ParamMove::Mov(src) = pm { *src == dst } else { false }
            })
        });

        if let Some(pos) = safe_pos {
            let (di, action) = moves.remove(pos);
            let dst = X86Operand::Reg(param_regs[di].clone());
            match action {
                ParamMove::Lea(src) => generator.asm.push(X86Instr::Lea(dst, src)),
                ParamMove::Mov(src) => {
                    if src != dst {
                        generator.asm.push(X86Instr::Mov(dst, src));
                    }
                }
            }
        } else {
            // Cycle among Mov entries: break by saving the first Mov src to rax
            let cycle_pos = moves.iter().position(|(_, pm)| matches!(pm, ParamMove::Mov(_)));
            if let Some(pos) = cycle_pos {
                if let ParamMove::Mov(cycle_src) = &moves[pos].1 {
                    let cycle_src = cycle_src.clone();
                    let rax = X86Operand::Reg(X86Reg::Rax);
                    generator.asm.push(X86Instr::Mov(rax.clone(), cycle_src.clone()));
                    // Any remaining Mov that reads cycle_src now reads rax
                    for (_, pm) in moves.iter_mut() {
                        if let ParamMove::Mov(src) = pm {
                            if *src == cycle_src {
                                *src = rax.clone();
                            }
                        }
                    }
                }
            } else {
                break; // Should not happen: no Mov entries but no safe position?
            }
        }
    }
}

pub fn gen_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, name: &str, args: &[Operand]) {
    let convention = get_convention(generator.target.calling_convention);
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();

    // Collect all integer param-register assignments into the parallel-move queue.
    // Float args are emitted immediately (xmm regs don't conflict with int param regs).
    let mut int_moves: Vec<(usize, ParamMove)> = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        let is_float = match arg {
            Operand::FloatConstant(_) => true,
            Operand::Var(v) => {
                generator.var_types.get(v).map_or(false, |t| matches!(t, Type::Float | Type::Double))
            }
            _ => false,
        };

        if i < param_regs.len() {
            if is_float && i < float_regs.len() {
                // Float args: emit immediately (xmm regs don't conflict with int param regs)
                let label = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Movss(X86Operand::Reg(float_regs[i].clone()), label));
            } else if !is_float {
                // Check for alloca var → needs LEA to get address
                let mut handled = false;
                if let Operand::Var(var) = arg {
                    if let Some(off) = generator.alloca_buffers.get(var) {
                        int_moves.push((i, ParamMove::Lea(X86Operand::Mem(X86Reg::Rbp, *off))));
                        handled = true;
                    }
                }
                if !handled {
                    if let Operand::Global(gname) = arg {
                        int_moves.push((i, ParamMove::Lea(X86Operand::RipRelLabel(gname.clone()))));
                    } else {
                        let src = generator.operand_to_op(arg);
                        int_moves.push((i, ParamMove::Mov(src)));
                    }
                }
            }
        } else {
            // Stack-passed args: emit immediately (no conflict with param regs)
            let offset = shadow_space + (i - param_regs.len()) * 8;
            if is_float {
                let label = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                generator.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                let mut handled = false;
                if let Operand::Var(var) = arg {
                    if let Some(off) = generator.alloca_buffers.get(var) {
                        generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *off)));
                        handled = true;
                    }
                }
                if !handled {
                    if let Operand::Global(gname) = arg {
                        generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(gname.clone())));
                    } else {
                        let val = generator.operand_to_op(arg);
                        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                    }
                }
                generator.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }

    // Emit integer param-register moves using cycle-safe parallel algorithm
    emit_parallel_int_moves(generator, &param_regs, int_moves);

    generator.asm.push(X86Instr::Call(name.to_string()));

    if let Some(d) = dest {
        let returns_float = generator.func_return_types.get(name)
            .map_or(false, |ret_type| matches!(ret_type, Type::Float | Type::Double));

        if returns_float {
            generator.var_types.insert(*d, Type::Float);
        }

        let d_op = generator.var_to_op(*d);
        if returns_float {
            generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
        } else {
            generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }
}



pub fn gen_indirect_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, func_ptr: &Operand, args: &[Operand]) {
    let convention = get_convention(generator.target.calling_convention);
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();
    
    let fp_op = generator.operand_to_op(func_ptr);
    generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), fp_op));
    
    for (i, arg) in args.iter().enumerate() {
        let is_float = match arg {
            Operand::FloatConstant(_) => true,
            Operand::Var(v) => {
                generator.var_types.get(v).map_or(false, |t| matches!(t, Type::Float | Type::Double))
            }
            _ => false,
        };

        if i < param_regs.len() {
            if is_float && i < float_regs.len() {
                let label = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Movss(X86Operand::Reg(float_regs[i].clone()), label));
            } else if !is_float {
                let val = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(param_regs[i].clone()), val));
            }
        } else {
            let offset = shadow_space + (i - param_regs.len()) * 8;
            if is_float {
                let label = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), label));
                generator.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Xmm0)));
            } else {
                let val = generator.operand_to_op(arg);
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                generator.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }
    
    generator.asm.push(X86Instr::CallIndirect(X86Operand::Reg(X86Reg::R10)));
    
    if let Some(d) = dest {
         let mut is_float_ret = false;
         
         // Try to infer return type from function pointer type
         if let Operand::Var(v) = func_ptr {
             if let Some(t) = generator.var_types.get(v) {
                 if let Type::FunctionPointer { return_type, .. } = t {
                     if matches!(**return_type, Type::Float | Type::Double) {
                         is_float_ret = true;
                     }
                     // Store the inferred type for the destination variable
                     generator.var_types.insert(*d, *return_type.clone());
                 }
             }
         }

        // Fallback to checking destination type if already known
        if !is_float_ret {
            if let Some(t) = generator.var_types.get(d) {
                if matches!(t, Type::Float | Type::Double) {
                    is_float_ret = true;
                }
            }
        }
        
        if is_float_ret {
            generator.var_types.insert(*d, Type::Float);
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Movss(dest_op, X86Operand::Reg(X86Reg::Xmm0)));
        } else {
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
    }
}
