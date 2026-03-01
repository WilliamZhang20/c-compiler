use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};

use model::Type;
use ir::{VarId, Operand};

/// A pending assignment to a param register.
enum ParamMove {
    /// emit: lea param_reg, operand
    Lea(X86Operand),
    /// emit: mov param_reg, operand
    Mov(X86Operand),
}

// ─── Shared helpers ─────────────────────────────────────────────

/// Classify an argument as (is_float, is_double).
fn classify_arg(generator: &FunctionGenerator, arg: &Operand) -> (bool, bool) {
    let is_float = match arg {
        Operand::FloatConstant(_) => true,
        Operand::Var(v) => generator.var_types.get(v)
            .map_or(false, |t| matches!(t, Type::Float | Type::Double)),
        _ => false,
    };
    let is_double = match arg {
        Operand::Var(v) => generator.var_types.get(v)
            .map_or(false, |t| matches!(t, Type::Double)),
        _ => false,
    };
    (is_float, is_double)
}

/// Resolve an integer argument to its X86 operand, distinguishing allocas and globals
/// (which need LEA to produce an address) from regular values (which use MOV).
fn resolve_int_arg(generator: &mut FunctionGenerator, arg: &Operand) -> ParamMove {
    if let Operand::Var(var) = arg {
        if let Some(&off) = generator.alloca_buffers.get(var) {
            return ParamMove::Lea(X86Operand::Mem(X86Reg::Rbp, off));
        }
    }
    if let Operand::Global(gname) = arg {
        return ParamMove::Lea(X86Operand::RipRelLabel(gname.clone()));
    }
    ParamMove::Mov(generator.operand_to_op(arg))
}

/// Marshal all arguments into registers and stack slots.
/// Float args are emitted immediately; integer param-register assignments are
/// collected and returned for cycle-safe parallel-move resolution.
fn marshal_args(
    generator: &mut FunctionGenerator,
    args: &[Operand],
    param_regs: &[X86Reg],
    float_regs: &[X86Reg],
    shadow_space: usize,
) -> Vec<(usize, ParamMove)> {
    let mut int_moves = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        let (is_float, is_double) = classify_arg(generator, arg);

        if i < param_regs.len() {
            if is_float && i < float_regs.len() {
                let op = generator.operand_to_op(arg);
                if is_double {
                    generator.asm.push(X86Instr::Movsd(X86Operand::Reg(float_regs[i].clone()), op));
                } else {
                    generator.asm.push(X86Instr::Movss(X86Operand::Reg(float_regs[i].clone()), op));
                }
            } else if !is_float {
                int_moves.push((i, resolve_int_arg(generator, arg)));
            }
        } else {
            // Stack-passed arguments
            let offset = (shadow_space + (i - param_regs.len()) * 8) as i32;
            if is_float {
                let op = generator.operand_to_op(arg);
                if is_double {
                    generator.asm.push(X86Instr::Movsd(X86Operand::Reg(X86Reg::Xmm0), op));
                    generator.asm.push(X86Instr::Movsd(
                        X86Operand::DoubleMem(X86Reg::Rsp, offset), X86Operand::Reg(X86Reg::Xmm0)));
                } else {
                    generator.asm.push(X86Instr::Movss(X86Operand::Reg(X86Reg::Xmm0), op));
                    generator.asm.push(X86Instr::Movss(
                        X86Operand::FloatMem(X86Reg::Rsp, offset), X86Operand::Reg(X86Reg::Xmm0)));
                }
            } else {
                let rax = X86Operand::Reg(X86Reg::Rax);
                match resolve_int_arg(generator, arg) {
                    ParamMove::Lea(src) => generator.asm.push(X86Instr::Lea(rax.clone(), src)),
                    ParamMove::Mov(src) => generator.asm.push(X86Instr::Mov(rax.clone(), src)),
                }
                generator.asm.push(X86Instr::Mov(X86Operand::Mem(X86Reg::Rsp, offset), rax));
            }
        }
    }

    int_moves
}

/// Emit all integer param-register assignments using a cycle-safe parallel-move algorithm.
/// `moves` is a list of (param_reg_index, ParamMove) pairs.
/// rax is used as a scratch register to break cycles.
fn emit_parallel_int_moves(generator: &mut FunctionGenerator,
                           param_regs: &[X86Reg],
                           mut moves: Vec<(usize, ParamMove)>) {
    loop {
        if moves.is_empty() { break; }

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
            let cycle_pos = moves.iter().position(|(_, pm)| matches!(pm, ParamMove::Mov(_)));
            if let Some(pos) = cycle_pos {
                if let ParamMove::Mov(cycle_src) = &moves[pos].1 {
                    let cycle_src = cycle_src.clone();
                    let rax = X86Operand::Reg(X86Reg::Rax);
                    generator.asm.push(X86Instr::Mov(rax.clone(), cycle_src.clone()));
                    for (_, pm) in moves.iter_mut() {
                        if let ParamMove::Mov(src) = pm {
                            if *src == cycle_src {
                                *src = rax.clone();
                            }
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}

/// Store a call's return value into the destination variable.
fn store_call_result(generator: &mut FunctionGenerator, dest: VarId, ret_type: Option<&Type>) {
    let is_float = ret_type.map_or(false, |t| matches!(t, Type::Float | Type::Double));
    let is_double = ret_type.map_or(false, |t| matches!(t, Type::Double));

    if is_float {
        if let Some(rt) = ret_type {
            generator.var_types.insert(dest, rt.clone());
        }
    }

    let d_op = generator.var_to_op(dest);
    if is_double {
        generator.asm.push(X86Instr::Movsd(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    } else if is_float {
        generator.asm.push(X86Instr::Movss(d_op, X86Operand::Reg(X86Reg::Xmm0)));
    } else {
        generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
    }
}

// ─── Public call generators ─────────────────────────────────────

pub fn gen_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, name: &str, args: &[Operand]) {
    let convention = generator.convention();
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();

    let int_moves = marshal_args(generator, args, &param_regs, &float_regs, shadow_space);
    emit_parallel_int_moves(generator, &param_regs, int_moves);

    generator.asm.push(X86Instr::Call(name.to_string()));

    if let Some(d) = dest {
        let ret_type = generator.func_return_types.get(name).cloned();
        store_call_result(generator, *d, ret_type.as_ref());
    }
}

pub fn gen_indirect_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, func_ptr: &Operand, args: &[Operand]) {
    let convention = generator.convention();
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();

    // Load function pointer into R10 (not a param reg, safe from arg marshalling)
    let fp_op = generator.operand_to_op(func_ptr);
    if let X86Operand::Label(name) = &fp_op {
        generator.asm.push(X86Instr::Lea(X86Operand::Reg(X86Reg::R10), X86Operand::RipRelLabel(name.clone())));
    } else {
        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::R10), fp_op));
    }

    let int_moves = marshal_args(generator, args, &param_regs, &float_regs, shadow_space);
    emit_parallel_int_moves(generator, &param_regs, int_moves);

    generator.asm.push(X86Instr::CallIndirect(X86Operand::Reg(X86Reg::R10)));

    if let Some(d) = dest {
        // Infer return type from function pointer type, global name, or dest type
        let ret_type = infer_indirect_return_type(generator, func_ptr, *d);
        store_call_result(generator, *d, ret_type.as_ref());
    }
}

/// Infer the return type of an indirect call from its function pointer.
fn infer_indirect_return_type(generator: &mut FunctionGenerator, func_ptr: &Operand, dest: VarId) -> Option<Type> {
    // 1. From function pointer variable's type annotation
    if let Operand::Var(v) = func_ptr {
        if let Some(t) = generator.var_types.get(v).cloned() {
            if let Type::FunctionPointer { return_type, .. } = &t {
                generator.var_types.insert(dest, *return_type.clone());
                return Some(*return_type.clone());
            }
        }
    }
    // 2. From global function name (after copy propagation)
    if let Operand::Global(name) = func_ptr {
        if let Some(ret_ty) = generator.func_return_types.get(name).cloned() {
            generator.var_types.insert(dest, ret_ty.clone());
            return Some(ret_ty);
        }
    }
    // 3. Fallback: destination type if already known
    generator.var_types.get(&dest).cloned()
}
