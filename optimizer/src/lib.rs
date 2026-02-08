use ir::{IRProgram, Function, Instruction, Operand, VarId};
use model::{BinaryOp, UnaryOp};
use std::collections::HashMap;

// Helper function to check if a number is a power of 2
fn is_power_of_two(n: i64) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

pub fn optimize(program: IRProgram) -> IRProgram {
    let mut program = program;
    for func in &mut program.functions {
        algebraic_simplification(func);
        strength_reduce_function(func);
        copy_propagation(func);
        common_subexpression_elimination(func);
        // DSE temporarily disabled - it was too aggressive
        // dead_store_elimination(func);
        optimize_function(func);
    }
    program
}

// Algebraic simplification: apply algebraic identities to simplify expressions
// Examples: x*0=0, x*1=x, x+0=x, x-0=x, x&0=0, x|0=x, etc.
fn algebraic_simplification(func: &mut Function) {
    for block in &mut func.blocks {
        let mut new_instructions = Vec::new();
        
        for inst in block.instructions.drain(..) {
            match inst {
                Instruction::Binary { dest, op, left, right } => {
                    let mut simplified = false;
                    
                    // Check for algebraic identities
                    match op {
                        BinaryOp::Mul => {
                            // x * 0 = 0
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                            // x * 1 = x
                            else if matches!(right, Operand::Constant(1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            // 0 * x = 0
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                            // 1 * x = x
                            else if matches!(left, Operand::Constant(1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: right.clone(),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::Div => {
                            // x / 1 = x
                            if matches!(right, Operand::Constant(1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            // 0 / x = 0 (assuming x != 0, which we can't verify at compile time)
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::Mod => {
                            // x % 1 = 0
                            if matches!(right, Operand::Constant(1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                            // 0 % x = 0
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::Add => {
                            // x + 0 = x
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            // 0 + x = x
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: right.clone(),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::Sub => {
                            // x - 0 = x
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            // 0 - x = -x (would need unary negate)
                            // Skip for now
                        }
                        BinaryOp::BitwiseAnd => {
                            // x & 0 = 0
                            if matches!(right, Operand::Constant(0)) || matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                            // x & -1 = x (all bits set)
                            else if matches!(right, Operand::Constant(-1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            else if matches!(left, Operand::Constant(-1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: right.clone(),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::BitwiseOr => {
                            // x | 0 = x
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: right.clone(),
                                });
                                simplified = true;
                            }
                            // x | -1 = -1 (all bits set)
                            else if matches!(right, Operand::Constant(-1)) || matches!(left, Operand::Constant(-1)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(-1),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::BitwiseXor => {
                            // x ^ 0 = x
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: right.clone(),
                                });
                                simplified = true;
                            }
                        }
                        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
                            // x << 0 = x, x >> 0 = x
                            if matches!(right, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: left.clone(),
                                });
                                simplified = true;
                            }
                            // 0 << x = 0, 0 >> x = 0
                            else if matches!(left, Operand::Constant(0)) {
                                new_instructions.push(Instruction::Copy {
                                    dest,
                                    src: Operand::Constant(0),
                                });
                                simplified = true;
                            }
                        }
                        _ => {}
                    }
                    
                    if !simplified {
                        new_instructions.push(Instruction::Binary { dest, op, left, right });
                    }
                }
                other => new_instructions.push(other),
            }
        }
        
        block.instructions = new_instructions;
    }
}

// Copy propagation: replace uses of copies with their sources
fn copy_propagation(func: &mut Function) {
    let mut copies: HashMap<VarId, Operand> = HashMap::new();
    
    // Find all simple copies
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { dest, src } = inst {
                // Track this copy
                copies.insert(*dest, src.clone());
            }
        }
    }
    
    // Replace uses with sources
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            match inst {
                Instruction::Binary { left, right, .. } => {
                    replace_operand(left, &copies);
                    replace_operand(right, &copies);
                }
                Instruction::Unary { src, .. } => {
                    replace_operand(src, &copies);
                }
                Instruction::Store { addr, src } => {
                    replace_operand(addr, &copies);
                    replace_operand(src, &copies);
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    replace_operand(base, &copies);
                    replace_operand(index, &copies);
                }
                Instruction::Call { args, .. } => {
                    for arg in args {
                        replace_operand(arg, &copies);
                    }
                }
                _ => {}
            }
        }
        
        // Handle terminator
        if let ir::Terminator::CondBr { cond, .. } = &mut block.terminator {
            replace_operand(cond, &copies);
        } else if let ir::Terminator::Ret(Some(op)) = &mut block.terminator {
            replace_operand(op, &copies);
        }
    }
}

fn replace_operand(op: &mut Operand, copies: &HashMap<VarId, Operand>) {
    if let Operand::Var(v) = op {
        if let Some(replacement) = copies.get(v) {
            *op = replacement.clone();
        }
    }
}

// Common subexpression elimination: eliminate redundant calculations
fn common_subexpression_elimination(func: &mut Function) {
    use std::collections::HashMap;
    
    // Map from (op, left, right) to the variable holding the result
    #[derive(Hash, Eq, PartialEq, Clone)]
    enum ExprKey {
        Binary(String, String, String),  // (op, left_str, right_str)
        Unary(String, String),            // (op, src_str)
    }
    
    let mut expr_map: HashMap<ExprKey, VarId> = HashMap::new();
    let mut var_replacements: HashMap<VarId, VarId> = HashMap::new();
    
    for block in &mut func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Binary { dest, op, left, right } => {
                    let key = ExprKey::Binary(
                        format!("{:?}", op),
                        format!("{:?}", left),
                        format!("{:?}", right),
                    );
                    
                    if let Some(&existing_var) = expr_map.get(&key) {
                        // Found a duplicate! Remember to replace dest with existing_var
                        var_replacements.insert(*dest, existing_var);
                    } else {
                        expr_map.insert(key, *dest);
                    }
                }
                Instruction::Unary { dest, op, src } => {
                    let key = ExprKey::Unary(
                        format!("{:?}", op),
                        format!("{:?}", src),
                    );
                    
                    if let Some(&existing_var) = expr_map.get(&key) {
                        var_replacements.insert(*dest, existing_var);
                    } else {
                        expr_map.insert(key, *dest);
                    }
                }
                _ => {}
            }
        }
    }
    
    // Now replace all uses of eliminated variables
    for block in &mut func.blocks {
        for inst in &mut block.instructions {
            match inst {
                Instruction::Binary { dest, left, right, .. } => {
                    if let Operand::Var(v) = left {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *left = Operand::Var(replacement);
                        }
                    }
                    if let Operand::Var(v) = right {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *right = Operand::Var(replacement);
                        }
                    }
                    // Also check if this instruction's dest should be replaced
                    if var_replacements.contains_key(dest) {
                        // This dest is redundant, convert to a copy
                        let _replacement = var_replacements[dest];
                        // We'll handle this by marking these as copies
                    }
                }
                Instruction::Unary { src, .. } => {
                    if let Operand::Var(v) = src {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *src = Operand::Var(replacement);
                        }
                    }
                }
                Instruction::Store { addr, src } => {
                    if let Operand::Var(v) = addr {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *addr = Operand::Var(replacement);
                        }
                    }
                    if let Operand::Var(v) = src {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *src = Operand::Var(replacement);
                        }
                    }
                }
                Instruction::Copy { src, .. } => {
                    if let Operand::Var(v) = src {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *src = Operand::Var(replacement);
                        }
                    }
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    if let Operand::Var(v) = base {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *base = Operand::Var(replacement);
                        }
                    }
                    if let Operand::Var(v) = index {
                        if let Some(&replacement) = var_replacements.get(v) {
                            *index = Operand::Var(replacement);
                        }
                    }
                }
                Instruction::Call { args, .. } => {
                    for arg in args {
                        if let Operand::Var(v) = arg {
                            if let Some(&replacement) = var_replacements.get(v) {
                                *arg = Operand::Var(replacement);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        // Also update terminators
        use ir::Terminator;
        match &mut block.terminator {
            Terminator::Ret(Some(op)) => {
                if let Operand::Var(v) = op {
                    if let Some(&replacement) = var_replacements.get(v) {
                        *op = Operand::Var(replacement);
                    }
                }
            }
            Terminator::CondBr { cond, .. } => {
                if let Operand::Var(v) = cond {
                    if let Some(&replacement) = var_replacements.get(v) {
                        *cond = Operand::Var(replacement);
                    }
                }
            }
            _ => {}
        }
    }
}

// Note: Dead store elimination removed - was too aggressive.
// TODO: Reimplement with proper liveness analysis that distinguishes
// between unused temporaries and meaningful stores.

// Strength reduction: replace expensive operations with cheaper equivalents
fn strength_reduce_function(func: &mut Function) {
    for block in &mut func.blocks {
        let mut new_instructions = Vec::new();
        
        for inst in block.instructions.drain(..) {
            match inst {
                Instruction::Binary { dest, op, left, right } => {
                    let mut transformed = false;
                    
                    match op {
                        // Multiply by power of 2 -> shift left
                        BinaryOp::Mul => {
                            if let Operand::Constant(c) = right {
                                if is_power_of_two(c) {
                                    let shift_amount = c.trailing_zeros() as i64;
                                    new_instructions.push(Instruction::Binary {
                                        dest,
                                        op: BinaryOp::ShiftLeft,
                                        left: left.clone(),
                                        right: Operand::Constant(shift_amount),
                                    });
                                    transformed = true;
                                }
                            } else if let Operand::Constant(c) = left {
                                if is_power_of_two(c) {
                                    let shift_amount = c.trailing_zeros() as i64;
                                    new_instructions.push(Instruction::Binary {
                                        dest,
                                        op: BinaryOp::ShiftLeft,
                                        left: right.clone(),
                                        right: Operand::Constant(shift_amount),
                                    });
                                    transformed = true;
                                }
                            }
                        }
                        // Divide by power of 2 -> shift right
                        BinaryOp::Div => {
                            if let Operand::Constant(c) = right {
                                if is_power_of_two(c) {
                                    let shift_amount = c.trailing_zeros() as i64;
                                    new_instructions.push(Instruction::Binary {
                                        dest,
                                        op: BinaryOp::ShiftRight,
                                        left: left.clone(),
                                        right: Operand::Constant(shift_amount),
                                    });
                                    transformed = true;
                                }
                            }
                        }
                        // Mod by power of 2 -> bitwise and
                        BinaryOp::Mod => {
                            if let Operand::Constant(c) = right {
                                if is_power_of_two(c) {
                                    new_instructions.push(Instruction::Binary {
                                        dest,
                                        op: BinaryOp::BitwiseAnd,
                                        left: left.clone(),
                                        right: Operand::Constant(c - 1),
                                    });
                                    transformed = true;
                                }
                            }
                        }
                        _ => {}
                    }
                    
                    // If transformed, skip to next instruction
                    if transformed {
                        continue;
                    }
                    
                    // Otherwise keep the original instruction
                    new_instructions.push(Instruction::Binary { dest, op, left, right });
                }
                _ => new_instructions.push(inst),
            }
        }
        
        block.instructions = new_instructions;
    }
}


fn optimize_function(func: &mut Function) {
    let mut changed = true;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10;
    
    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;
        let mut constants: HashMap<VarId, i64> = HashMap::new();

        for block in &mut func.blocks {
            let mut new_instructions = Vec::new();
            for inst in block.instructions.drain(..) {
                match inst {
                    Instruction::Binary { dest, op, left, right } => {
                        let l = resolve_operand(&left, &constants);
                        let r = resolve_operand(&right, &constants);
                        if let (Operand::Constant(lc), Operand::Constant(rc)) = (&l, &r) {
                            if let Some(val) = fold_binary(op.clone(), *lc, *rc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy { dest, src: Operand::Constant(val) });
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::Binary { dest, op, left: l, right: r });
                    }
                    Instruction::Unary { dest, op, src } => {
                        let s = resolve_operand(&src, &constants);
                        if let Operand::Constant(sc) = s {
                            if let Some(val) = fold_unary(op.clone(), sc) {
                                constants.insert(dest, val);
                                new_instructions.push(Instruction::Copy { dest, src: Operand::Constant(val) });
                                changed = true;
                                continue;
                            }
                        }
                        new_instructions.push(Instruction::Unary { dest, op, src: s });
                    }
                    Instruction::Copy { dest, src } => {
                        let s = resolve_operand(&src, &constants);
                        if let Operand::Constant(sc) = s {
                            constants.insert(dest, sc);
                        }
                        new_instructions.push(Instruction::Copy { dest, src: s });
                    }
                    Instruction::Call { dest, name, args } => {
                        let mut resolved_args = Vec::new();
                        for arg in args {
                            resolved_args.push(resolve_operand(&arg, &constants));
                        }
                        new_instructions.push(Instruction::Call { dest, name, args: resolved_args });
                    }
                    Instruction::Load { dest, addr } => {
                        new_instructions.push(Instruction::Load {
                            dest,
                            addr: resolve_operand(&addr, &constants),
                        });
                    }
                    Instruction::Store { addr, src } => {
                        new_instructions.push(Instruction::Store {
                            addr: resolve_operand(&addr, &constants),
                            src: resolve_operand(&src, &constants),
                        });
                    }
                    Instruction::GetElementPtr { dest, base, index, element_type } => {
                        new_instructions.push(Instruction::GetElementPtr {
                            dest,
                            base: resolve_operand(&base, &constants),
                            index: resolve_operand(&index, &constants),
                            element_type,
                        });
                    }
                    _ => new_instructions.push(inst),
                }
            }
            block.instructions = new_instructions;

            // Also fold terminator
            match &mut block.terminator {
                ir::Terminator::CondBr { cond, then_block, else_block } => {
                    let c = resolve_operand(cond, &constants);
                    if let Operand::Constant(val) = c {
                        let target = if val != 0 { *then_block } else { *else_block };
                        block.terminator = ir::Terminator::Br(target);
                        changed = true;
                    } else {
                        *cond = c;
                    }
                }
                ir::Terminator::Ret(Some(op)) => {
                    *op = resolve_operand(op, &constants);
                }
                _ => {}
            }
        }
        
        changed |= dce_function(func);
    }
    
    if iterations >= MAX_ITERATIONS {
        eprintln!("Warning: Optimizer reached max iterations ({}) for function {}", MAX_ITERATIONS, func.name);
    }
}

fn dce_function(func: &mut Function) -> bool {
    let mut changed = false;
    let mut used_vars = std::collections::HashSet::new();
    
    // Find all used variables
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Binary { left, right, .. } => {
                    if let Operand::Var(v) = left { used_vars.insert(*v); }
                    if let Operand::Var(v) = right { used_vars.insert(*v); }
                }
                Instruction::Unary { src, .. } => {
                    if let Operand::Var(v) = src { used_vars.insert(*v); }
                }
                Instruction::Copy { src, .. } => {
                    if let Operand::Var(v) = src { used_vars.insert(*v); }
                }
                Instruction::Call { args, .. } => {
                    for arg in args {
                        if let Operand::Var(v) = arg { used_vars.insert(*v); }
                    }
                }
                Instruction::IndirectCall { func_ptr, args, .. } => {
                    if let Operand::Var(v) = func_ptr { used_vars.insert(*v); }
                    for arg in args {
                        if let Operand::Var(v) = arg { used_vars.insert(*v); }
                    }
                }
                Instruction::Load { addr, .. } => {
                    if let Operand::Var(v) = addr { used_vars.insert(*v); }
                }
                Instruction::Store { addr, src } => {
                    if let Operand::Var(v) = addr { used_vars.insert(*v); }
                    if let Operand::Var(v) = src { used_vars.insert(*v); }
                }
                Instruction::GetElementPtr { base, index, element_type: _, .. } => {
                    if let Operand::Var(v) = base { used_vars.insert(*v); }
                    if let Operand::Var(v) = index { used_vars.insert(*v); }
                }
                Instruction::Alloca { .. } => {}
                Instruction::Phi { dest: _, preds } => {
                    for (_, v) in preds {
                        used_vars.insert(*v);
                    }
                }
            }
        }
        match &block.terminator {
            ir::Terminator::CondBr { cond, .. } => {
                if let Operand::Var(v) = cond { used_vars.insert(*v); }
            }
            ir::Terminator::Ret(Some(op)) => {
                if let Operand::Var(v) = op { used_vars.insert(*v); }
            }
            _ => {}
        }
    }

    // Remove instructions that define unused variables and have no side effects
    for block in &mut func.blocks {
        let initial_count = block.instructions.len();
        block.instructions.retain(|inst| {
            match inst {
                Instruction::Binary { dest, .. } |
                Instruction::Unary { dest, .. } |
                Instruction::Copy { dest, .. } |
                Instruction::Load { dest, .. } |
                Instruction::GetElementPtr { dest, .. } |
                Instruction::Phi { dest, .. } => {
                    used_vars.contains(dest)
                }
                Instruction::Call { .. } |
                Instruction::IndirectCall { .. } |
                Instruction::Store { .. } |
                Instruction::Alloca { .. } => true, // Side effects or essential
            }
        });
        if block.instructions.len() < initial_count {
            changed = true;
        }
    }
    
    changed
}

fn resolve_operand(op: &Operand, constants: &HashMap<VarId, i64>) -> Operand {
    match op {
        Operand::Var(v) => {
            if let Some(c) = constants.get(v) {
                Operand::Constant(*c)
            } else {
                op.clone()
            }
        }
        _ => op.clone(),
    }
}

fn fold_binary(op: BinaryOp, l: i64, r: i64) -> Option<i64> {
    match op {
        BinaryOp::Add => Some(l + r),
        BinaryOp::Sub => Some(l - r),
        BinaryOp::Mul => Some(l * r),
        BinaryOp::Div => if r != 0 { Some(l / r) } else { None },
        BinaryOp::EqualEqual => Some((l == r) as i64),
        BinaryOp::NotEqual => Some((l != r) as i64),
        BinaryOp::Less => Some((l < r) as i64),
        BinaryOp::LessEqual => Some((l <= r) as i64),
        BinaryOp::Greater => Some((l > r) as i64),
        BinaryOp::GreaterEqual => Some((l >= r) as i64),
        BinaryOp::Mod => if r != 0 { Some(l % r) } else { None },
        BinaryOp::BitwiseAnd => Some(l & r),
        BinaryOp::BitwiseOr => Some(l | r),
        BinaryOp::BitwiseXor => Some(l ^ r),
        BinaryOp::ShiftLeft => Some(l << r),
        BinaryOp::ShiftRight => Some(l >> r),
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::Assign => None,
    }
}

fn fold_unary(op: UnaryOp, s: i64) -> Option<i64> {
    match op {
        UnaryOp::Minus => Some(-s),
        UnaryOp::Plus => Some(s),
        UnaryOp::LogicalNot => Some((s == 0) as i64),
        UnaryOp::BitwiseNot => Some(!s),
        UnaryOp::AddrOf | UnaryOp::Deref => None,
    }
}
