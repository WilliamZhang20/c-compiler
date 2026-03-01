use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};
use model::Type;
use ir::{VarId, Operand};

/// Returns (is_float, is_double, use_byte, use_word, use_dword, is_unsigned) for a type.
/// is_float covers both float and double (use is_double to distinguish).
fn type_load_info(value_type: &Type) -> (bool, bool, bool, bool, bool, bool) {
    match value_type {
        Type::Float  => (true, false, false, false, true, false),
        Type::Double => (true, true, false, false, false, false),
        Type::Char   => (false, false, true, false, false, false),
        Type::UnsignedChar => (false, false, true, false, false, true),
        Type::Short  => (false, false, false, true, false, false),
        Type::UnsignedShort => (false, false, false, true, false, true),
        Type::Int | Type::UnsignedInt => (false, false, false, false, true, matches!(value_type, Type::UnsignedInt)),
        Type::Typedef(name) => match name.as_str() {
            "int8_t"  | "int8"  => (false, false, true, false, false, false),
            "uint8_t" | "uint8" => (false, false, true, false, false, true),
            "int16_t" | "int16" => (false, false, false, true, false, false),
            "uint16_t"| "uint16"=> (false, false, false, true, false, true),
            "int32_t" | "int32" => (false, false, false, false, true, false),
            "uint32_t"| "uint32"=> (false, false, false, false, true, true),
            _ => (false, false, false, false, false, false),
        },
        _ => (false, false, false, false, false, false),
    }
}

/// Emit a float/double load from memory into d_op via xmm0
fn emit_fp_load(generator: &mut FunctionGenerator, d_op: X86Operand, is_double: bool, base: X86Reg, offset: i32) {
    if is_double {
        generator.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), X86Operand::DoubleMem(base, offset)));
        generator.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    } else {
        generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::FloatMem(base, offset)));
        generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    }
}

/// Emit a float/double store from xmm0 to memory
fn emit_fp_store(generator: &mut FunctionGenerator, is_double: bool, base: X86Reg, offset: i32) {
    if is_double {
        generator.asm.push(X86Instr::Movsd(X86Operand::DoubleMem(base, offset), X86Operand::Reg(X86Reg::Xmm0)));
    } else {
        generator.asm.push(X86Instr::Movss(X86Operand::FloatMem(base, offset), X86Operand::Reg(X86Reg::Xmm0)));
    }
}

pub fn gen_load(generator: &mut FunctionGenerator, dest: VarId, addr: &Operand, value_type: &Type) {
    generator.var_types.insert(dest, value_type.clone());
    let d_op = generator.var_to_op(dest);
    let (is_float, is_double, use_byte, use_word, use_dword, is_unsigned) = type_load_info(value_type);

    // Optimization: if loading directly from an alloca (stack slot), just read it
    if let Operand::Var(var) = addr {
         if let Some(buffer_offset) = generator.alloca_buffers.get(var) {
             if is_float {
                 emit_fp_load(generator, d_op, is_double, X86Reg::Rbp, *buffer_offset);
             } else if use_byte {
                 if is_unsigned {
                     generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rbp, *buffer_offset)));
                 } else {
                     generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rbp, *buffer_offset)));
                 }
                 generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
             } else if use_word {
                 if is_unsigned {
                     generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::WordMem(X86Reg::Rbp, *buffer_offset)));
                 } else {
                     generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::WordMem(X86Reg::Rbp, *buffer_offset)));
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
             if is_double {
                 generator.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), X86Operand::GlobalQwordMem(name.clone())));
                 generator.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
             } else {
                 generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), X86Operand::GlobalMem(name.clone())));
                 generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
             }
         } else {
             if use_dword {
                 generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Eax), X86Operand::GlobalMem(name.clone())));
                 generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Eax)));
             } else {
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
        emit_fp_load(generator, d_op, is_double, X86Reg::Rax, 0);
    } else if use_byte {
         if is_unsigned {
             generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rax, 0)));
         } else {
             generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::ByteMem(X86Reg::Rax, 0)));
         }
         generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    } else if use_word {
         if is_unsigned {
             generator.asm.push(X86Instr::Movzx(X86Operand::Reg(X86Reg::Rax), X86Operand::WordMem(X86Reg::Rax, 0)));
         } else {
             generator.asm.push(X86Instr::Movsx(X86Operand::Reg(X86Reg::Rax), X86Operand::WordMem(X86Reg::Rax, 0)));
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
    let (is_float, is_double, use_byte, use_word, use_dword, _is_unsigned) = type_load_info(value_type);

    // Load src into register
    if is_float {
        let s_op = generator.operand_to_op(src);
         match s_op {
             X86Operand::Reg(X86Reg::Xmm0) => {},
             _ => {
                 if is_double {
                     generator.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), s_op));
                 } else {
                     generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), s_op));
                 }
             }
         }
    } else {
        let s_op = generator.operand_to_op(src);
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
    generator.load_address_into(addr, X86Reg::Rax);
    
    // Store
    if is_float {
        emit_fp_store(generator, is_double, X86Reg::Rax, 0);
    } else if use_byte {
        generator.asm.push(X86Instr::Mov(X86Operand::ByteMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Cl)));
    } else if use_word {
        generator.asm.push(X86Instr::Mov(X86Operand::WordMem(X86Reg::Rax, 0), X86Operand::Reg(X86Reg::Cx)));
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
    
    generator.load_address_into(base, X86Reg::Rax);

    generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), i_op));
    if elem_size != 1 {
        generator.asm.push(X86Instr::Imul(X86Operand::Reg(X86Reg::Rcx), X86Operand::Imm(elem_size)));
    }
    generator.asm.push(X86Instr::Add(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
    generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
}
