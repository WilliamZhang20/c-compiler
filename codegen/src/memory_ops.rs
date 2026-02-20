use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::Type;
use ir::{VarId, Operand};

/// Returns (is_float, use_byte, use_word, use_dword, is_unsigned) for a type.
/// use_byte=1-byte, use_word=2-byte, use_dword=4-byte, else 8-byte.
fn type_load_info(value_type: &Type) -> (bool, bool, bool, bool, bool) {
    match value_type {
        Type::Float  => (true, false, false, true, false),
        Type::Double => (true, false, false, false, false),
        Type::Char   => (false, true, false, false, false),
        Type::UnsignedChar => (false, true, false, false, true),
        Type::Short  => (false, false, true, false, false),
        Type::UnsignedShort => (false, false, true, false, true),
        Type::Int | Type::UnsignedInt => (false, false, false, true, matches!(value_type, Type::UnsignedInt)),
        Type::Typedef(name) => match name.as_str() {
            "int8_t"  | "int8"  => (false, true, false, false, false),
            "uint8_t" | "uint8" => (false, true, false, false, true),
            "int16_t" | "int16" => (false, false, true, false, false),
            "uint16_t"| "uint16"=> (false, false, true, false, true),
            "int32_t" | "int32" => (false, false, false, true, false),
            "uint32_t"| "uint32"=> (false, false, false, true, true),
            _ => (false, false, false, false, false), // 8-byte default
        },
        _ => (false, false, false, false, false), // 8-byte / pointer
    }
}

pub fn gen_load(generator: &mut FunctionGenerator, dest: VarId, addr: &Operand, value_type: &Type) {
    generator.var_types.insert(dest, value_type.clone());
    let d_op = generator.var_to_op(dest);
    let (is_float, use_byte, use_word, use_dword, is_unsigned) = type_load_info(value_type);
    let _ = use_word; // word-size loads use the same dword path for now

    
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
    let (is_float, use_byte, _use_word, use_dword, _is_unsigned) = type_load_info(value_type);
    let _ = _use_word;
    let _ = _is_unsigned;

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
