use crate::function::FunctionGenerator;
use crate::x86::{X86Instr, X86Operand, X86Reg};

use model::Type;
use ir::{VarId, Operand};

// ─── SysV AMD64 struct classification ───────────────────────────

/// How a struct argument should be passed per SysV AMD64 ABI.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StructArgClass {
    /// Struct fits in 1 GP register (size ≤ 8 bytes)
    OneReg,
    /// Struct fits in 2 GP registers (8 < size ≤ 16 bytes)
    TwoReg,
    /// Struct must be passed in memory (size > 16 bytes)
    Memory,
}

/// Classify a struct for SysV AMD64 argument passing.
/// Returns None if the type is not a struct/union, Some(class) otherwise.
pub(crate) fn classify_struct_arg(generator: &FunctionGenerator, ty: &Type) -> Option<StructArgClass> {
    let size = match ty {
        Type::Struct(name) => {
            if let Some(s_def) = generator.structs.get(name) {
                let is_packed = s_def.attributes.iter()
                    .any(|attr| matches!(attr, model::Attribute::Packed));
                model::TypeLayout::new(generator.structs, generator.unions).struct_size(s_def, is_packed)
            } else {
                return None;
            }
        }
        Type::Union(name) => {
            if let Some(u_def) = generator.unions.get(name) {
                u_def.fields.iter()
                    .map(|f| model::TypeLayout::new(generator.structs, generator.unions).size_of(&f.field_type))
                    .max()
                    .unwrap_or(0)
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(if size <= 8 {
        StructArgClass::OneReg
    } else if size <= 16 {
        StructArgClass::TwoReg
    } else {
        StructArgClass::Memory
    })
}

/// Get the size of a struct/union type using the layout calculator.
fn get_aggregate_size(generator: &FunctionGenerator, ty: &Type) -> usize {
    model::TypeLayout::new(generator.structs, generator.unions).size_of(ty)
}

/// Pre-process call arguments for SysV AMD64 struct by-value passing.
/// Small structs (≤16 bytes) are decomposed into 1-2 qword loads;
/// large structs (>16 bytes) are passed by pointer (address).
/// Returns a new flattened arg list and the emitted instructions to load struct eightbytes.
fn flatten_struct_args(
    generator: &mut FunctionGenerator,
    args: &[Operand],
) -> Vec<Operand> {
    let mut flat_args = Vec::new();

    for arg in args {
        let arg_type = match arg {
            Operand::Var(v) => generator.var_types.get(v).cloned(),
            _ => None,
        };

        let class = arg_type.as_ref().and_then(|ty| classify_struct_arg(generator, ty));

        match class {
            Some(StructArgClass::OneReg) => {
                // Load the struct's first (and only) eightbyte into a temp var
                if let Operand::Var(var) = arg {
                    if let Some(&off) = generator.alloca_buffers.get(var) {
                        // Load from the alloca buffer
                        let temp = generator.new_temp_var();
                        let slot = generator.get_or_create_slot(temp);
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Reg(X86Reg::Rax),
                            X86Operand::Mem(X86Reg::Rbp, off),
                        ));
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Mem(X86Reg::Rbp, slot),
                            X86Operand::Reg(X86Reg::Rax),
                        ));
                        flat_args.push(Operand::Var(temp));
                        continue;
                    }
                }
                // Fallback: pass as-is (pointer)
                flat_args.push(arg.clone());
            }
            Some(StructArgClass::TwoReg) => {
                // Load both eightbytes into temp vars
                if let Operand::Var(var) = arg {
                    if let Some(&off) = generator.alloca_buffers.get(var) {
                        let temp1 = generator.new_temp_var();
                        let slot1 = generator.get_or_create_slot(temp1);
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Reg(X86Reg::Rax),
                            X86Operand::Mem(X86Reg::Rbp, off),
                        ));
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Mem(X86Reg::Rbp, slot1),
                            X86Operand::Reg(X86Reg::Rax),
                        ));

                        let temp2 = generator.new_temp_var();
                        let slot2 = generator.get_or_create_slot(temp2);
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Reg(X86Reg::Rax),
                            X86Operand::Mem(X86Reg::Rbp, off + 8),
                        ));
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Mem(X86Reg::Rbp, slot2),
                            X86Operand::Reg(X86Reg::Rax),
                        ));

                        flat_args.push(Operand::Var(temp1));
                        flat_args.push(Operand::Var(temp2));
                        continue;
                    }
                }
                flat_args.push(arg.clone());
            }
            Some(StructArgClass::Memory) | None => {
                // Large struct → pass by pointer (existing behavior: LEA the alloca)
                // Non-struct → pass as-is
                flat_args.push(arg.clone());
            }
        }
    }

    flat_args
}

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
    // Check for struct return type
    if let Some(ty) = ret_type {
        if let Some(class) = classify_struct_arg(generator, ty) {
            let size = get_aggregate_size(generator, ty);
            generator.var_types.insert(dest, ty.clone());
            // Struct return values are stored into the dest alloca buffer
            if let Some(&off) = generator.alloca_buffers.get(&dest) {
                match class {
                    StructArgClass::OneReg => {
                        // Return value in RAX → store to alloca
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Mem(X86Reg::Rbp, off),
                            X86Operand::Reg(X86Reg::Rax),
                        ));
                    }
                    StructArgClass::TwoReg => {
                        // Return value in RAX:RDX → store both eightbytes
                        generator.asm.push(X86Instr::Mov(
                            X86Operand::Mem(X86Reg::Rbp, off),
                            X86Operand::Reg(X86Reg::Rax),
                        ));
                        if size > 8 {
                            generator.asm.push(X86Instr::Mov(
                                X86Operand::Mem(X86Reg::Rbp, off + 8),
                                X86Operand::Reg(X86Reg::Rdx),
                            ));
                        }
                    }
                    StructArgClass::Memory => {
                        // For MEMORY-class returns, the hidden pointer mechanism
                        // means the callee wrote directly to our alloca via RDI.
                        // Nothing to do here.
                    }
                }
                return;
            }
            // No alloca — just store RAX as fallback
            let d_op = generator.var_to_op(dest);
            generator.asm.push(X86Instr::Mov(d_op, X86Operand::Reg(X86Reg::Rax)));
            return;
        }
    }

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

// ─── Bit-count builtin codegen ────────────────────────────────────

#[derive(Copy, Clone)]
enum BitWidth {
    W32,
    W64,
}

impl BitWidth {
    fn zero_result(self) -> i64 {
        match self {
            BitWidth::W32 => 32,
            BitWidth::W64 => 64,
        }
    }

    fn clz_xor(self) -> i64 {
        match self {
            BitWidth::W32 => 31,
            BitWidth::W64 => 63,
        }
    }

    fn result_reg(self) -> X86Reg {
        match self {
            BitWidth::W32 => X86Reg::Eax,
            BitWidth::W64 => X86Reg::Rax,
        }
    }

    fn cmov_scratch(self) -> X86Reg {
        match self {
            BitWidth::W32 => X86Reg::Ecx,
            BitWidth::W64 => X86Reg::Rcx,
        }
    }
}

fn bit_width_for_builtin(name: &str) -> Option<(BitWidth, &str)> {
    match name {
        "__builtin_clz" | "__builtin_ctz" | "__builtin_popcount" => Some((BitWidth::W32, name)),
        "__builtin_clzl" | "__builtin_ctzl" | "__builtin_popcountl" => Some((BitWidth::W64, name)),
        "__builtin_clzll" | "__builtin_ctzll" | "__builtin_popcountll" => Some((BitWidth::W64, name)),
        _ => None,
    }
}

fn u32_operand(op: X86Operand) -> X86Operand {
    match op {
        X86Operand::Reg(r) => X86Operand::Reg(r.to_32bit()),
        other => other,
    }
}

fn store_result_reg_if_needed(generator: &mut FunctionGenerator, dest: VarId, result: X86Reg) {
    let dest_op = generator.var_to_op(dest);
    let result_op = X86Operand::Reg(result.clone());
    let same = dest_op == result_op
        || matches!(
            (&dest_op, &result),
            (X86Operand::Reg(X86Reg::Rax), X86Reg::Eax)
                | (X86Operand::Reg(X86Reg::Eax), X86Reg::Rax)
                | (X86Operand::Reg(X86Reg::Rcx), X86Reg::Ecx)
                | (X86Operand::Reg(X86Reg::Ecx), X86Reg::Rcx)
        );
    if same {
        return;
    }

    // 32-bit result in eax → 64-bit GPR: write low 32 bits (zero-extends into full reg).
    if result == X86Reg::Eax {
        if let X86Operand::Reg(dest_reg) = &dest_op {
            let dest32 = dest_reg.to_32bit();
            if dest32 != X86Reg::Eax {
                generator.asm.push(X86Instr::Mov(
                    X86Operand::Reg(dest32),
                    X86Operand::Reg(X86Reg::Eax),
                ));
                return;
            }
        }
    }

    generator.asm.push(X86Instr::Mov(dest_op, result_op));
}

/// Branch-free clz/ctz/popcount matching GCC -O2/O3 instruction selection.
fn gen_bit_count_builtin(
    generator: &mut FunctionGenerator,
    dest: VarId,
    kind: &str,
    width: BitWidth,
    src_op: X86Operand,
) {
    if let X86Operand::Imm(v) = src_op {
        let u = v as u64;
        let bits = match width {
            BitWidth::W32 => 32u32,
            BitWidth::W64 => 64,
        };
        let folded = match kind {
            "__builtin_clz" | "__builtin_clzl" | "__builtin_clzll" => {
                if u == 0 {
                    bits as i64
                } else {
                    match width {
                        BitWidth::W32 => (u as u32).leading_zeros() as i64,
                        BitWidth::W64 => u.leading_zeros() as i64,
                    }
                }
            }
            "__builtin_ctz" | "__builtin_ctzl" | "__builtin_ctzll" => {
                if u == 0 {
                    bits as i64
                } else {
                    match width {
                        BitWidth::W32 => (u as u32).trailing_zeros() as i64,
                        BitWidth::W64 => u.trailing_zeros() as i64,
                    }
                }
            }
            "__builtin_popcount" | "__builtin_popcountl" | "__builtin_popcountll" => match width {
                BitWidth::W32 => (u as u32).count_ones() as i64,
                BitWidth::W64 => u.count_ones() as i64,
            },
            _ => unreachable!(),
        };
        let dest_op = generator.var_to_op(dest);
        generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Imm(folded)));
        return;
    }

    let result = width.result_reg();
    let scratch = width.cmov_scratch();
    let src = match width {
        BitWidth::W32 => u32_operand(src_op),
        BitWidth::W64 => src_op,
    };

    match kind {
        "__builtin_clz" | "__builtin_clzl" | "__builtin_clzll" => {
            generator.asm.push(X86Instr::Raw(format!(
                "  bsr {}, {}",
                result.to_str(),
                src
            )));
            generator.asm.push(X86Instr::Raw(format!(
                "  xor {}, {}",
                result.to_str(),
                width.clz_xor()
            )));
            generator.asm.push(X86Instr::Mov(
                X86Operand::Reg(scratch.clone()),
                X86Operand::Imm(width.zero_result()),
            ));
            generator.asm.push(X86Instr::Raw(format!(
                "  cmovz {}, {}",
                result.to_str(),
                scratch.to_str()
            )));
        }
        "__builtin_ctz" | "__builtin_ctzl" | "__builtin_ctzll" => {
            generator.asm.push(X86Instr::Raw(format!(
                "  bsf {}, {}",
                result.to_str(),
                src
            )));
            generator.asm.push(X86Instr::Mov(
                X86Operand::Reg(scratch.clone()),
                X86Operand::Imm(width.zero_result()),
            ));
            generator.asm.push(X86Instr::Raw(format!(
                "  cmovz {}, {}",
                result.to_str(),
                scratch.to_str()
            )));
        }
        "__builtin_popcount" | "__builtin_popcountl" | "__builtin_popcountll" => {
            generator.asm.push(X86Instr::Raw(format!(
                "  popcnt {}, {}",
                result.to_str(),
                src
            )));
        }
        _ => unreachable!(),
    }

    store_result_reg_if_needed(generator, dest, result);
}

fn gen_bswap_builtin(
    generator: &mut FunctionGenerator,
    dest: VarId,
    name: &str,
    src_op: X86Operand,
) {
    let dest_op = generator.var_to_op(dest);
    match name {
        "__builtin_bswap16" => {
            if !matches!(dest_op, X86Operand::Reg(X86Reg::Ax)) {
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Ax), src_op));
                generator.asm.push(X86Instr::Raw("  rol ax, 8".to_string()));
                if dest_op != X86Operand::Reg(X86Reg::Ax) {
                    generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Ax)));
                }
            } else if dest_op != src_op {
                generator.asm.push(X86Instr::Mov(dest_op.clone(), src_op));
                generator.asm.push(X86Instr::Raw("  rol ax, 8".to_string()));
            } else {
                generator.asm.push(X86Instr::Raw("  rol ax, 8".to_string()));
            }
        }
        "__builtin_bswap32" | "__builtin_bswap64" => {
            let work = X86Reg::Rax;
            let work_op = X86Operand::Reg(work.clone());
            if work_op != src_op {
                generator.asm.push(X86Instr::Mov(work_op.clone(), src_op));
            }
            let insn = if name == "__builtin_bswap64" {
                "  bswap rax"
            } else {
                "  bswap eax"
            };
            generator.asm.push(X86Instr::Raw(insn.to_string()));
            store_result_reg_if_needed(generator, dest, work);
        }
        _ => unreachable!(),
    }
}

// ─── Public call generators ─────────────────────────────────────

pub fn gen_call(generator: &mut FunctionGenerator, dest: &Option<VarId>, name: &str, args: &[Operand]) {
    // Intercept __builtin_clz / ctz / popcount (32- and 64-bit variants).
    if let Some((width, kind)) = bit_width_for_builtin(name) {
        if args.len() == 1 {
            if let Some(d) = dest {
                let src_op = generator.operand_to_op(&args[0]);
                gen_bit_count_builtin(generator, *d, kind, width, src_op);
            }
            return;
        }
    }

    // Intercept __builtin_bswap* — emit inline bswap instruction
    if (name == "__builtin_bswap16" || name == "__builtin_bswap32" || name == "__builtin_bswap64")
        && args.len() == 1
    {
        if let Some(d) = dest {
            let src_op = generator.operand_to_op(&args[0]);
            gen_bswap_builtin(generator, *d, name, src_op);
        }
        return;
    }

    // Intercept __sync_synchronize — emit mfence
    if name == "__sync_synchronize" {
        generator.asm.push(X86Instr::Raw("mfence".to_string()));
        return;
    }

    // Intercept __sync_val_compare_and_swap(ptr, old, new) → lock cmpxchg
    if name == "__sync_val_compare_and_swap" && args.len() == 3 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let old_op = generator.operand_to_op(&args[1]);
            let new_op = generator.operand_to_op(&args[2]);
            // old → rax, new → rcx, ptr → rdx
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), old_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), new_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Raw("lock cmpxchg [rdx], rcx".to_string()));
            // Result (old value) is in rax
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __sync_lock_test_and_set(ptr, val) → xchg
    if name == "__sync_lock_test_and_set" && args.len() == 2 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let val_op = generator.operand_to_op(&args[1]);
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Raw("xchg [rdx], rax".to_string()));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __sync_lock_release(ptr) → mov [ptr], 0 + mfence
    if name == "__sync_lock_release" && args.len() == 1 {
        let ptr_op = generator.operand_to_op(&args[0]);
        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
        generator.asm.push(X86Instr::Raw("mov qword [rdx], 0".to_string()));
        generator.asm.push(X86Instr::Raw("mfence".to_string()));
        return;
    }

    // Intercept __sync_fetch_and_add/sub(ptr, val) → lock xadd / lock sub+mov
    if (name == "__sync_fetch_and_add" || name == "__sync_fetch_and_sub") && args.len() == 2 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let val_op = generator.operand_to_op(&args[1]);
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val_op));
            if name == "__sync_fetch_and_sub" {
                generator.asm.push(X86Instr::Raw("neg rax".to_string()));
            }
            generator.asm.push(X86Instr::Raw("lock xadd [rdx], rax".to_string()));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __sync_fetch_and_{and,or,xor}(ptr, val) → CAS loop
    if (name == "__sync_fetch_and_and" || name == "__sync_fetch_and_or" || name == "__sync_fetch_and_xor") && args.len() == 2 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let val_op = generator.operand_to_op(&args[1]);
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), val_op));
            // CAS loop: rax = [rdx], rcx = val
            generator.asm.push(X86Instr::Raw("mov rax, [rdx]".to_string()));
            let label = format!(".Lsync_cas_{}", d.0);
            generator.asm.push(X86Instr::Raw(format!("{}:", label)));
            generator.asm.push(X86Instr::Raw("mov rsi, rax".to_string()));
            let op_str = match name {
                "__sync_fetch_and_and" => "and",
                "__sync_fetch_and_or" => "or",
                "__sync_fetch_and_xor" => "xor",
                _ => unreachable!(),
            };
            generator.asm.push(X86Instr::Raw(format!("{} rsi, rcx", op_str)));
            generator.asm.push(X86Instr::Raw("lock cmpxchg [rdx], rsi".to_string()));
            generator.asm.push(X86Instr::Raw(format!("jne {}", label)));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __atomic_load_n(ptr, memorder) → mov from [ptr]
    if name == "__atomic_load_n" && args.len() == 2 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Raw("mov rax, [rdx]".to_string()));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __atomic_store_n(ptr, val, memorder) → mov to [ptr]
    if name == "__atomic_store_n" && args.len() == 3 {
        let ptr_op = generator.operand_to_op(&args[0]);
        let val_op = generator.operand_to_op(&args[1]);
        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
        generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val_op));
        generator.asm.push(X86Instr::Raw("mov [rdx], rax".to_string()));
        if let Some(d) = dest {
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __atomic_compare_exchange_n(ptr, expected, desired, weak, succ_order, fail_order) → lock cmpxchg
    if name == "__atomic_compare_exchange_n" && args.len() >= 4 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let expected_op = generator.operand_to_op(&args[1]);
            let desired_op = generator.operand_to_op(&args[2]);
            // expected is a pointer — load expected value from it 
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rsi), expected_op));
            generator.asm.push(X86Instr::Raw("mov rax, [rsi]".to_string()));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), desired_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Raw("lock cmpxchg [rdx], rcx".to_string()));
            // On failure, store actual value back to *expected
            generator.asm.push(X86Instr::Raw("mov [rsi], rax".to_string()));
            // Result: 1 if success (ZF set), 0 if failure
            generator.asm.push(X86Instr::Raw("sete al".to_string()));
            generator.asm.push(X86Instr::Raw("movzx rax, al".to_string()));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __atomic_exchange_n(ptr, val, memorder) → xchg
    if name == "__atomic_exchange_n" && args.len() == 3 {
        if let Some(d) = dest {
            let ptr_op = generator.operand_to_op(&args[0]);
            let val_op = generator.operand_to_op(&args[1]);
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), val_op));
            generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
            generator.asm.push(X86Instr::Raw("xchg [rdx], rax".to_string()));
            let dest_op = generator.var_to_op(*d);
            generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
        }
        return;
    }

    // Intercept __atomic_fetch_{add,sub,and,or,xor}(ptr, val, memorder) 
    if name.starts_with("__atomic_fetch_") && args.len() == 3 {
        let op_name = &name["__atomic_fetch_".len()..];
        if matches!(op_name, "add" | "sub" | "and" | "or" | "xor") {
            if let Some(d) = dest {
                let ptr_op = generator.operand_to_op(&args[0]);
                let val_op = generator.operand_to_op(&args[1]);
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rdx), ptr_op));
                generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rcx), val_op));
                if op_name == "add" || op_name == "sub" {
                    generator.asm.push(X86Instr::Mov(X86Operand::Reg(X86Reg::Rax), X86Operand::Reg(X86Reg::Rcx)));
                    if op_name == "sub" {
                        generator.asm.push(X86Instr::Raw("neg rax".to_string()));
                    }
                    generator.asm.push(X86Instr::Raw("lock xadd [rdx], rax".to_string()));
                } else {
                    // CAS loop for and/or/xor
                    generator.asm.push(X86Instr::Raw("mov rax, [rdx]".to_string()));
                    let label = format!(".Latomic_cas_{}", d.0);
                    generator.asm.push(X86Instr::Raw(format!("{}:", label)));
                    generator.asm.push(X86Instr::Raw("mov rsi, rax".to_string()));
                    generator.asm.push(X86Instr::Raw(format!("{} rsi, rcx", op_name)));
                    generator.asm.push(X86Instr::Raw("lock cmpxchg [rdx], rsi".to_string()));
                    generator.asm.push(X86Instr::Raw(format!("jne {}", label)));
                }
                let dest_op = generator.var_to_op(*d);
                generator.asm.push(X86Instr::Mov(dest_op, X86Operand::Reg(X86Reg::Rax)));
            }
            return;
        }
    }

    let convention = generator.convention();
    let param_regs = convention.param_regs();
    let float_regs = convention.float_param_regs();
    let shadow_space = convention.shadow_space_size();

    // Flatten struct args: decompose small structs into register-sized values
    let flat_args = flatten_struct_args(generator, args);

    let int_moves = marshal_args(generator, &flat_args, &param_regs, &float_regs, shadow_space);
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

    // Flatten struct args: decompose small structs into register-sized values
    let flat_args = flatten_struct_args(generator, args);

    let int_moves = marshal_args(generator, &flat_args, &param_regs, &float_regs, shadow_space);
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
