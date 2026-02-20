use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use crate::calling_convention::get_convention;
use model::Type;
use ir::{VarId, Operand};

pub fn gen_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, name: &str, args: &[Operand]) {
    let convention = get_convention(generator.target.calling_convention);
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();
    
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
                let mut handled = false;
                if let Operand::Var(var) = arg {
                     if let Some(off) = generator.alloca_buffers.get(var) {
                         generator.asm.push(X86Instr::Lea(X86Operand::Reg(param_regs[i].clone()), X86Operand::Mem(X86Reg::Rbp, *off)));
                         handled = true;
                     }
                }
                if !handled {
                    if let Operand::Global(name) = arg {
                        generator.asm.push(X86Instr::Lea(X86Operand::Reg(param_regs[i].clone()), X86Operand::RipRelLabel(name.clone())));
                    } else {
                        let val = generator.operand_to_op(arg);
                        generator.asm.push(X86Instr::Mov(X86Operand::Reg(param_regs[i].clone()), val));
                    }
                }
            }
        } else {
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
                    if let Operand::Global(name) = arg {
                        generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
                    } else {
                        let val = generator.operand_to_op(arg);
                        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val));
                    }
                }
                generator.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset as i32), X86Operand::Reg(X86Reg::Rax)));
            }
        }
    }
    
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
