// Detect and eliminate linear sum recurrences of the form:
//
//   if (n <= T) return base(n);
//   return f(n - d1) + f(n - d2) + ... + f(n - dk);
//
// where `base(n)` is the parameter itself or a constant, and each `di` is a
// positive compile-time constant. Fibonacci is the k=2 case with d=[1,2],
// T=1, base(n)=n. Rewritten to an O(n) rolling-buffer loop.

use ir::{
    BasicBlock, BlockId, BranchHint, Function, Instruction, Operand, Terminator, VarId,
};
use model::{BinaryOp, Type};
use std::collections::HashSet;

fn max_var_id(func: &Function) -> usize {
    func.params
        .iter()
        .map(|(_, v)| v.0)
        .chain(
            func.blocks
                .iter()
                .flat_map(|b| b.instructions.iter().filter_map(|i| i.dest().map(|d| d.0))),
        )
        .max()
        .unwrap_or(0)
}

struct IdGen {
    next_var: usize,
}

impl IdGen {
    fn new(func: &Function) -> Self {
        Self {
            next_var: max_var_id(func) + 1,
        }
    }

    fn var(&mut self, func: &mut Function) -> VarId {
        let v = VarId(self.next_var);
        self.next_var += 1;
        func.var_types.insert(v, Type::Int);
        v
    }
}

/// How the base-case arm returns.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BaseReturn {
    Param,
    Constant(i64),
}

/// A detected sum recurrence over one scalar induction parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SumRecurrence {
    param: VarId,
    base_threshold: i64,
    base_return: BaseReturn,
    /// Positive constants `d` in self-calls `f(n - d)`, sorted ascending.
    offsets: Vec<i64>,
}

fn resolves_to_param(op: &Operand, param: VarId, scope: &[Instruction]) -> bool {
    match op {
        Operand::Var(v) if *v == param => true,
        Operand::Var(v) => scope.iter().any(|i| {
            matches!(i, Instruction::Copy { dest, src: Operand::Var(p) } if *dest == *v && *p == param)
        }),
        _ => false,
    }
}

/// Parse `n <= T` or equivalent `n < T+1` on the entry block.
fn parse_base_threshold(entry: &BasicBlock, param: VarId) -> Option<i64> {
    let cond = match &entry.terminator {
        Terminator::CondBr { cond, .. } => cond,
        _ => return None,
    };
    let Operand::Var(cmp_var) = cond else {
        return None;
    };

    for inst in &entry.instructions {
        match inst {
            Instruction::Binary {
                dest,
                op: BinaryOp::LessEqual,
                left,
                right: Operand::Constant(t),
            } if dest == cmp_var => {
                if resolves_to_param(left, param, &entry.instructions) {
                    return Some(*t);
                }
            }
            Instruction::Binary {
                dest,
                op: BinaryOp::Less,
                left,
                right: Operand::Constant(t),
            } if dest == cmp_var => {
                if resolves_to_param(left, param, &entry.instructions) && *t > 0 {
                    return Some(t - 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn block_returns_base(block: &BasicBlock, param: VarId) -> Option<BaseReturn> {
    match &block.terminator {
        Terminator::Ret(Some(Operand::Var(v))) if *v == param => Some(BaseReturn::Param),
        Terminator::Ret(Some(Operand::Var(v))) => {
            for inst in &block.instructions {
                if let Instruction::Copy {
                    dest,
                    src: Operand::Var(p),
                } = inst
                {
                    if *dest == *v && *p == param {
                        return Some(BaseReturn::Param);
                    }
                }
            }
            None
        }
        Terminator::Ret(Some(Operand::Constant(c))) => Some(BaseReturn::Constant(*c)),
        _ => None,
    }
}

fn skip_empty_goto(func: &Function, start: BlockId) -> BlockId {
    let mut cur = start;
    for _ in 0..4 {
        let Some(block) = func.blocks.get(cur.0) else {
            break;
        };
        match &block.terminator {
            Terminator::Br(next) if block.instructions.is_empty() => cur = *next,
            _ => break,
        }
    }
    cur
}

fn self_calls_in_block<'a>(
    block: &'a BasicBlock,
    name: &str,
    param: VarId,
) -> Vec<(&'a Instruction, i64)> {
    block
        .instructions
        .iter()
        .filter_map(|inst| {
            let Instruction::Call {
                name: n,
                args,
                dest: Some(_),
            } = inst
            else {
                return None;
            };
            if n != name || args.len() != 1 {
                return None;
            }
            let Operand::Var(arg_var) = &args[0] else {
                return None;
            };
            let offset = block.instructions.iter().find_map(|i| {
                if let Instruction::Binary {
                    dest,
                    op: BinaryOp::Sub,
                    left,
                    right: Operand::Constant(d),
                } = i
                {
                    if dest == arg_var
                        && *d > 0
                        && resolves_to_param(left, param, &block.instructions)
                    {
                        return Some(*d);
                    }
                }
                None
            })?;
            Some((inst, offset))
        })
        .collect()
}

/// Collect variables holding self-call results that are summed into the return value.
fn add_tree_leaves(block: &BasicBlock, root: VarId) -> Option<HashSet<VarId>> {
    let mut leaves = HashSet::new();
    let mut stack = vec![root];

    while let Some(cur) = stack.pop() {
        if block
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { dest: Some(d), .. } if *d == cur))
        {
            leaves.insert(cur);
            continue;
        }

        let mut expanded = false;
        for inst in &block.instructions {
            if let Instruction::Binary {
                dest,
                op: BinaryOp::Add,
                left: Operand::Var(a),
                right: Operand::Var(b),
            } = inst
            {
                if *dest == cur {
                    stack.push(*a);
                    stack.push(*b);
                    expanded = true;
                    break;
                }
            }
        }
        if !expanded {
            return None;
        }
    }

    Some(leaves)
}

fn detect_sum_recurrence(func: &Function) -> Option<SumRecurrence> {
    if func.params.len() != 1 || func.params[0].0 != Type::Int {
        return None;
    }
    let param = func.params[0].1;

    let entry = func.blocks.get(func.entry_block.0)?;
    let Terminator::CondBr {
        then_block,
        else_block,
        ..
    } = entry.terminator
    else {
        return None;
    };

    let base_threshold = parse_base_threshold(entry, param)?;
    let base_block = func.blocks.get(then_block.0)?;
    let base_return = block_returns_base(base_block, param)?;

    let rec_id = skip_empty_goto(func, else_block);
    let rec = func.blocks.get(rec_id.0)?;
    let calls = self_calls_in_block(rec, &func.name, param);
    if calls.len() < 2 {
        return None;
    }

    let mut offsets: Vec<i64> = calls.iter().map(|(_, d)| *d).collect();
    offsets.sort_unstable();
    offsets.dedup();
    if offsets.is_empty() {
        return None;
    }

    // Every self-call must use the same parameter minus a positive constant.
    for inst in &rec.instructions {
        if let Instruction::Call { name, args, .. } = inst {
            if name != &func.name {
                continue;
            }
            let Operand::Var(arg_var) = &args[0] else {
                return None;
            };
            let mut found = false;
            for other in &rec.instructions {
                if let Instruction::Binary {
                    dest,
                    op: BinaryOp::Sub,
                    left,
                    right: Operand::Constant(d),
                } = other
                {
                    if dest == arg_var
                        && *d > 0
                        && resolves_to_param(left, param, &rec.instructions)
                    {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return None;
            }
        }
    }

    let Terminator::Ret(Some(Operand::Var(ret))) = &rec.terminator else {
        return None;
    };
    let leaves = add_tree_leaves(rec, *ret)?;
    let call_dests: HashSet<VarId> = calls
        .iter()
        .filter_map(|(inst, _)| {
            if let Instruction::Call { dest: Some(d), .. } = inst {
                Some(*d)
            } else {
                None
            }
        })
        .collect();
    if leaves != call_dests {
        return None;
    }

    Some(SumRecurrence {
        param,
        base_threshold,
        base_return,
        offsets,
    })
}

fn base_value(pattern: &SumRecurrence, n: i64) -> i64 {
    match pattern.base_return {
        BaseReturn::Param => n,
        BaseReturn::Constant(c) => c,
    }
}

/// Compute f(0)..f(count-1) from the recurrence definition.
fn bootstrap(pattern: &SumRecurrence, count: usize) -> Option<Vec<i64>> {
    let mut values = vec![0i64; count];
    for n in 0..count {
        let n_i64 = n as i64;
        if n_i64 <= pattern.base_threshold {
            values[n] = base_value(pattern, n_i64);
        } else {
            let mut sum = 0i64;
            for &d in &pattern.offsets {
                let idx = n.checked_sub(d as usize)?;
                sum = sum.checked_add(values[idx])?;
            }
            values[n] = sum;
        }
    }
    Some(values)
}

fn rewrite_sum_recurrence(func: &mut Function, pattern: &SumRecurrence) {
    let m = *pattern.offsets.last().unwrap() as usize;
    let boot = bootstrap(pattern, m).expect("bootstrap failed after successful detect");

    let mut ids = IdGen::new(func);
    let entry = BlockId(0);
    let base = BlockId(1);
    let init = BlockId(2);
    let loop_cond = BlockId(3);
    let loop_body = BlockId(4);
    let done = BlockId(5);

    let cmp_le = ids.var(func);
    let cmp_loop = ids.var(func);
    let i_var = ids.var(func);
    let next = ids.var(func);
    let mut slots: Vec<VarId> = (0..m).map(|_| ids.var(func)).collect();

    // slots[0] = f(n-1), slots[k] = f(n-1-k) at loop head; init from bootstrap.
    let mut init_insts = Vec::new();
    for (slot_idx, &val) in boot.iter().enumerate().rev() {
        init_insts.push(Instruction::Copy {
            dest: slots[m - 1 - slot_idx],
            src: Operand::Constant(val),
        });
    }
    init_insts.push(Instruction::Copy {
        dest: i_var,
        src: Operand::Constant(m as i64),
    });

    let mut loop_body_insts = Vec::new();
    let mut acc = None::<VarId>;
    for &d in &pattern.offsets {
        let slot_idx = (d as usize).saturating_sub(1);
        debug_assert!(slot_idx < m);
        let term = slots[slot_idx];
        if let Some(prev) = acc {
            let sum = ids.var(func);
            loop_body_insts.push(Instruction::Binary {
                dest: sum,
                op: BinaryOp::Add,
                left: Operand::Var(prev),
                right: Operand::Var(term),
            });
            acc = Some(sum);
        } else {
            acc = Some(term);
        }
    }
    let acc = acc.expect("at least two offsets");
    loop_body_insts.push(Instruction::Copy {
        dest: next,
        src: Operand::Var(acc),
    });
    for idx in (1..m).rev() {
        loop_body_insts.push(Instruction::Copy {
            dest: slots[idx],
            src: Operand::Var(slots[idx - 1]),
        });
    }
    loop_body_insts.push(Instruction::Copy {
        dest: slots[0],
        src: Operand::Var(next),
    });
    loop_body_insts.push(Instruction::Binary {
        dest: i_var,
        op: BinaryOp::Add,
        left: Operand::Var(i_var),
        right: Operand::Constant(1),
    });

    let base_ret = match pattern.base_return {
        BaseReturn::Param => Operand::Var(pattern.param),
        BaseReturn::Constant(c) => Operand::Constant(c),
    };

    func.blocks = vec![
        BasicBlock {
            id: entry,
            instructions: vec![Instruction::Binary {
                dest: cmp_le,
                op: BinaryOp::LessEqual,
                left: Operand::Var(pattern.param),
                right: Operand::Constant(pattern.base_threshold),
            }],
            terminator: Terminator::CondBr {
                cond: Operand::Var(cmp_le),
                then_block: base,
                else_block: init,
                hint: BranchHint::None,
            },
            is_label_target: false,
        },
        BasicBlock {
            id: base,
            instructions: vec![],
            terminator: Terminator::Ret(Some(base_ret)),
            is_label_target: false,
        },
        BasicBlock {
            id: init,
            instructions: init_insts,
            terminator: Terminator::Br(loop_cond),
            is_label_target: false,
        },
        BasicBlock {
            id: loop_cond,
            instructions: vec![Instruction::Binary {
                dest: cmp_loop,
                op: BinaryOp::Greater,
                left: Operand::Var(i_var),
                right: Operand::Var(pattern.param),
            }],
            terminator: Terminator::CondBr {
                cond: Operand::Var(cmp_loop),
                then_block: done,
                else_block: loop_body,
                hint: BranchHint::None,
            },
            is_label_target: false,
        },
        BasicBlock {
            id: loop_body,
            instructions: loop_body_insts,
            terminator: Terminator::Br(loop_cond),
            is_label_target: false,
        },
        BasicBlock {
            id: done,
            instructions: vec![],
            terminator: Terminator::Ret(Some(Operand::Var(slots[0]))),
            is_label_target: false,
        },
    ];
    func.entry_block = entry;
}

pub fn eliminate_linear_recurrences(func: &mut Function) {
    if let Some(pattern) = detect_sum_recurrence(func) {
        rewrite_sum_recurrence(func, &pattern);
        // Re-run mem2reg: the iterative loop introduces loop-carried defs without phis.
        ir::mem2reg(func);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimize;

    fn compile_fn(source: &str, name: &str) -> ir::Function {
        let tokens = lexer::lex(source).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        let mut prog = lowerer.lower_program(&ast).unwrap();
        let func = prog
            .functions
            .iter_mut()
            .find(|f| f.name == name)
            .unwrap();
        ir::mem2reg(func);
        prog.functions.into_iter().find(|f| f.name == name).unwrap()
    }

    #[test]
    fn detects_fibonacci_as_sum_recurrence() {
        let fib = compile_fn(
            "int fib(int n) { if (n <= 1) return n; return fib(n-1) + fib(n-2); }",
            "fib",
        );
        let pattern = detect_sum_recurrence(&fib).unwrap();
        assert_eq!(pattern.base_threshold, 1);
        assert_eq!(pattern.base_return, BaseReturn::Param);
        assert_eq!(pattern.offsets, vec![1, 2]);
    }

    #[test]
    fn detects_tribonacci_shape() {
        let tri = compile_fn(
            "int tri(int n) {
                if (n <= 0) return 0;
                return tri(n-1) + tri(n-2) + tri(n-3);
            }",
            "tri",
        );
        let pattern = detect_sum_recurrence(&tri).unwrap();
        assert_eq!(pattern.base_threshold, 0);
        assert_eq!(pattern.base_return, BaseReturn::Constant(0));
        assert_eq!(pattern.offsets, vec![1, 2, 3]);
    }

    #[test]
    fn rejects_factorial() {
        let fact = compile_fn(
            "int fact(int n) { if (n <= 1) return 1; return n * fact(n-1); }",
            "fact",
        );
        assert!(detect_sum_recurrence(&fact).is_none());
    }

    #[test]
    fn rewrite_removes_self_calls() {
        let mut fib = compile_fn(
            "int fib(int n) { if (n <= 1) return n; return fib(n-1) + fib(n-2); }",
            "fib",
        );
        eliminate_linear_recurrences(&mut fib);
        assert!(!fib.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { name, .. } if name == "fib"))
        }));
    }

    #[test]
    fn fib_pipeline_has_no_recursion() {
        let src = include_str!("../../benchmarks/fib.c");
        let tokens = lexer::lex(src).unwrap();
        let ast = parser::parse_tokens(&tokens).unwrap();
        let mut lowerer = ir::Lowerer::new();
        let ir = optimize(lowerer.lower_program(&ast).unwrap());
        let fib_fn = ir.functions.iter().find(|f| f.name == "fib").unwrap();
        assert!(!fib_fn.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { name, .. } if name == "fib"))
        }));
    }
}
