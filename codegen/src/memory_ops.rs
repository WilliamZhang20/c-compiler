use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::Type;
use ir::{VarId, Operand};

pub fn gen_load(generator: &mut FunctionGenerator, dest: VarId, addr: &Operand, value_type: &Type) {
    generator.var_types.insert(dest, value_type.clone());
    let d_op = generator.var_to_op(dest);
    let is_float = matches!(value_type, Type::Float | Type::Double);
    let use_dword = matches!(value_type, Type::Int | Type::Float);
    let use_byte = matches!(value_type, Type::Char | Type::UnsignedChar);
    let is_unsigned = matches!(value_type, Type::UnsignedChar);
    
    // Optimization: if loading directly from an alloca (stack slot), just read it
    if let Operand::Var(var) = addr {
         if let Some(buffer_offset) = generator.alloca_buffers.get(var) {
             if is_float {
                 generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rbp, *buffer_offset)));
                 generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
             } else if use_byte {
                 if is_unsigned {
                     generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rbp, *buffer_offset)));
                 } else {
                     generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rbp, *buffer_offset)));
                 }
                 generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
             } else {
                 if use_dword {
                     generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rbp, *buffer_offset)));
                     generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
                 } else {
                     generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
                 }
                 generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
             }
             return;
         }
    }

    // Optimization: if loading directly from a global, use RIP-relative load
    if let Operand::Global(name) = addr {
         if is_float {
             generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::GlobalMem(name.clone())));
             generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
         } else {
             if use_dword {
                 // 32-bit int: Mov EAX, [rip+name]
                 generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::GlobalMem(name.clone())));
                 generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
             } else {
                 // 64-bit int/ptr: Mov RAX, [rip+name]
                 generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
             }
             generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
         }
         return;
    }
    
    // General case: Load address into RAX, then dereference
    if let Operand::Var(var) = addr {
         let v_op = generator.var_to_op(*var);
         generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), v_op));
    } else {
         let op = generator.operand_to_op(addr);
         match op {
             X86Operand::Label(l) | X86Operand::RipRelLabel(l) => {
                 generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l)));
             }
             _ => {
                 generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), op));
             }
         }
    }
    
    if is_float {
        generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(X86Reg::Rax, 0)));
        generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    } else if use_byte {
         if is_unsigned {
             generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rax, 0)));
         } else {
             generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rax, 0)));
         }
         generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    } else {
         if use_dword {
             generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::DwordMem(X86Reg::Rax, 0)));
             generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
         } else {
             generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rax, 0)));
         }
         generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    }
}

pub fn gen_store(generator: &mut FunctionGenerator, addr: &Operand, src: &Operand, value_type: &Type) {
    let is_float = matches!(value_type, Type::Float | Type::Double);
    let use_dword = matches!(value_type, Type::Int | Type::Float);
    let use_byte = matches!(value_type, Type::Char | Type::UnsignedChar);

    // Load src into register
    if is_float {
        let s_op = generator.operand_to_op(src);
         match s_op {
             X86Operand::Reg(X86Reg::Xmm0) => {},
             _ => generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op))
         }
    } else {
        let s_op = generator.operand_to_op(src);
         // Handle alloca address loading vs value loading
         if let Operand::Global(name) = src {
             generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::RipRelLabel(name.clone())));
         } else if let Operand::Var(v) = src {
             if let Some(off) = generator.alloca_buffers.get(v) {
                 generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rcx), X86Operand::Mem(X86Reg::Rbp, *off)));
             } else {
                 generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
             }
         } else {
             generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), s_op));
         }
    }

    // Load address into RAX
    if let Operand::Var(var) = addr {
         if let Some(buffer_offset) = generator.alloca_buffers.get(var) {
              generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
         } else {
             let b_op = generator.var_to_op(*var);
             generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
         }
    } else {
         let op = generator.operand_to_op(addr);
         if let X86Operand::Label(l) = &op {
             generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l.clone())));
         } else if let X86Operand::RipRelLabel(l) = &op {
             generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(l.clone())));
         } else {
             generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), op));
         }
    }
    
    // Store
    if is_float {
        generator.asm.push(X86Instr::Movss(X86Operand::FloatMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Xmm0)));
    } else if use_byte {
        generator.asm.push(X86Instr::Mov(X86Operand::ByteMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Cl)));
    } else if use_dword {
        generator.asm.push(X86Instr::Mov(X86Operand::DwordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Ecx)));
    } else {
        generator.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Rcx)));
    }
}

pub fn gen_gep(generator: &mut FunctionGenerator, dest: VarId, base: &Operand, index: &Operand, element_type: &Type) {
    let i_op = generator.operand_to_op(index);
    let d_op = generator.var_to_op(dest);
    let elem_size = generator.get_type_size(element_type) as i64;
    
    match base {
        Operand::Global(name) => {
            generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::RipRelLabel(name.clone())));
        }
        Operand::Var(var) => {
            if let Some(buffer_offset) = generator.alloca_buffers.get(var) {
                generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::Rax), X86Operand::Mem(X86Reg::Rbp, *buffer_offset)));
            } else {
                let b_op = generator.var_to_op(*var);
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), b_op));
            }
        }
        _ => {}
    }

    generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), i_op));
    if elem_size != 1 {
        generator.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rcx), X86Operand::Imm(elem_size)));
    }
    generator.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
    generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
}
