// Function inlining pass
//
// Inlines small functions at call sites to eliminate call overhead and
// enable further intraprocedural optimizations (constant propagation,
// dead code elimination, vectorization, etc.).
//
// Strategy:
// - Inline functions with ≤ MAX_INLINE_BLOCKS basic blocks
// - Don't inline recursive functions (call to self)
// - Don't inline variadic functions
// - Don't inline functions with inline asm
// - Inline at most MAX_INLINE_SITES call sites per function

use ir::{Function, BasicBlock, Instruction, Operand, VarId, BlockId, Terminator, IRProgram};
use std::collections::{HashMap, HashSet};

/// Maximum number of basic blocks in a function to consider it for inlining
const MAX_INLINE_BLOCKS: usize = 30;

/// Maximum number of call sites to inline per caller function
const MAX_INLINE_SITES: usize = 20;

/// Detect whether a function contains any loops (back-edges in the CFG).
/// Uses DFS to find back-edges: an edge to an ancestor in the DFS tree.
fn has_loop(func: &Function) -> bool {
    if func.blocks.is_empty() {
        return false;
    }

    let block_ids: Vec<BlockId> = func.blocks.iter().map(|b| b.id).collect();
    let id_to_idx: HashMap<BlockId, usize> = block_ids.iter().enumerate().map(|(i, &bid)| (bid, i)).collect();

    // Build adjacency list
    let mut successors: Vec<Vec<usize>> = vec![vec![]; func.blocks.len()];
    for (i, block) in func.blocks.iter().enumerate() {
        let targets: Vec<BlockId> = match &block.terminator {
            Terminator::Br(t) => vec![*t],
            Terminator::CondBr { then_block, else_block, .. } => vec![*then_block, *else_block],
            _ => vec![],
        };
        for t in targets {
            if let Some(&idx) = id_to_idx.get(&t) {
                successors[i].push(idx);
            }
        }
    }

    // DFS for back-edges
    let mut visited = vec![false; func.blocks.len()];
    let mut on_stack = vec![false; func.blocks.len()];
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)]; // (node, successor_index)
    visited[0] = true;
    on_stack[0] = true;

    while let Some((node, si)) = stack.last_mut() {
        if *si < successors[*node].len() {
            let next = successors[*node][*si];
            *si += 1;
            if on_stack[next] {
                return true; // Back-edge found → loop exists
            }
            if !visited[next] {
                visited[next] = true;
                on_stack[next] = true;
                stack.push((next, 0));
            }
        } else {
            on_stack[*node] = false;
            stack.pop();
        }
    }
    false
}

/// Check if a function is eligible for inlining
fn is_inlineable(func: &Function) -> bool {
    // Too large
    if func.blocks.len() > MAX_INLINE_BLOCKS {
        return false;
    }

    // Don't inline main
    if func.name == "main" {
        return false;
    }

    // Don't inline functions that contain loops — they bloat the caller
    // and hurt register allocation (inlined loops use more registers in
    // the caller's context, causing spills).
    if has_loop(func) {
        return false;
    }

    // Don't inline functions with inline asm or va_start
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::InlineAsm { .. } => return false,
                Instruction::VaStart { .. } => return false,
                _ => {}
            }
        }
    }

    true
}

/// Check if a function has recursive calls (calls itself)
fn is_recursive(func: &Function) -> bool {
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Call { name, .. } = inst {
                if *name == func.name {
                    return true;
                }
            }
        }
    }
    false
}

/// Inline all eligible call sites in the program.
/// Returns true if any inlining was performed.
pub fn inline_functions(program: &mut IRProgram) -> bool {
    // Build a map of inlineable function definitions
    let mut inline_candidates: HashMap<String, Function> = HashMap::new();
    for func in &program.functions {
        if is_inlineable(func) && !is_recursive(func) {
            inline_candidates.insert(func.name.clone(), func.clone());
        }
    }

    if inline_candidates.is_empty() {
        return false;
    }

    let mut any_inlined = false;

    // For each function, find call sites and inline them
    for func_idx in 0..program.functions.len() {
        let mut inlined_count = 0;

        // We need to iterate, because inlining may create new blocks with calls
        // But we only do one pass to avoid infinite expansion
        let mut block_idx = 0;
        while block_idx < program.functions[func_idx].blocks.len() {
            let mut inst_idx = 0;
            while inst_idx < program.functions[func_idx].blocks[block_idx].instructions.len() {
                if inlined_count >= MAX_INLINE_SITES {
                    break;
                }

                let should_inline = {
                    let inst = &program.functions[func_idx].blocks[block_idx].instructions[inst_idx];
                    if let Instruction::Call { name, .. } = inst {
                        inline_candidates.contains_key(name)
                    } else {
                        false
                    }
                };

                if should_inline {
                    let (call_name, call_dest, call_args) = {
                        let inst = &program.functions[func_idx].blocks[block_idx].instructions[inst_idx];
                        if let Instruction::Call { name, dest, args } = inst {
                            (name.clone(), dest.clone(), args.clone())
                        } else {
                            unreachable!()
                        }
                    };

                    let callee = inline_candidates.get(&call_name).unwrap().clone();
                    inline_call_site(
                        &mut program.functions[func_idx],
                        block_idx,
                        inst_idx,
                        &callee,
                        call_dest,
                        &call_args,
                    );
                    any_inlined = true;
                    inlined_count += 1;
                    // After inlining, the current block has been split.
                    // Don't advance inst_idx — the continuation block is a new block
                    // at the end of func.blocks. We'll process it when block_idx reaches it.
                    break; // Move to next block, this block's remaining instructions are in the continuation
                } else {
                    inst_idx += 1;
                }
            }
            block_idx += 1;
        }
    }

    any_inlined
}

/// Inline a single call site.
///
/// Transforms:
///   block_id: [ ...pre_call_insts..., Call(dest, name, args), ...post_call_insts..., terminator ]
///
/// Into:
///   block_id:   [ ...pre_call_insts..., param copies, Br(callee_entry) ]
///   callee blocks (remapped): [ ... body ... ]
///   return_merge_block: [ Copy(call_dest, return_value)?, ...post_call_insts..., terminator ]
fn inline_call_site(
    caller: &mut Function,
    block_idx: usize,
    inst_idx: usize,
    callee: &Function,
    call_dest: Option<VarId>,
    call_args: &[Operand],
) {
    // Compute ID offsets to avoid conflicts
    let max_block_id = caller.blocks.iter().map(|b| b.id.0).max().unwrap_or(0);
    let max_var_id = find_max_var(caller);

    let block_offset = max_block_id + 1;
    let var_offset = max_var_id + 1;

    // Create the return merge block ID
    let merge_block_id = BlockId(block_offset + callee.blocks.len());

    // Remember the original block ID for Phi fixup
    let orig_block_id = caller.blocks[block_idx].id;

    // Split the current block: everything after the call goes to merge_block
    let orig_block = &mut caller.blocks[block_idx];
    let post_call_insts: Vec<Instruction> = orig_block.instructions.split_off(inst_idx + 1);
    let orig_terminator = std::mem::replace(&mut orig_block.terminator, Terminator::Unreachable);

    // Remove the Call instruction itself
    orig_block.instructions.pop(); // removes the Call at inst_idx

    // Add parameter copies: callee param vars ← call args
    // NOTE: call_args are in the CALLER's namespace — do NOT remap them
    for (i, (_, param_var)) in callee.params.iter().enumerate() {
        if i < call_args.len() {
            let remapped_param = VarId(param_var.0 + var_offset);
            orig_block.instructions.push(Instruction::Copy {
                dest: remapped_param,
                src: call_args[i].clone(),
            });
        }
    }

    // Redirect pre-call block to callee entry
    let callee_entry = BlockId(callee.entry_block.0 + block_offset);
    orig_block.terminator = Terminator::Br(callee_entry);

    // Clone and remap callee blocks, collecting return site info for Phi construction
    let mut inlined_blocks: Vec<BasicBlock> = Vec::new();
    // Track return sites: (remapped_block_id, var_id_holding_return_value)
    let mut ret_sites: Vec<(BlockId, VarId)> = Vec::new();
    let mut next_temp_var = var_offset + find_max_var(callee) + 1;

    for callee_block in &callee.blocks {
        let mut new_block = BasicBlock {
            id: BlockId(callee_block.id.0 + block_offset),
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
            is_label_target: false,
        };

        // Remap instructions
        for inst in &callee_block.instructions {
            new_block.instructions.push(remap_instruction(inst, var_offset, block_offset));
        }

        // Remap terminator
        new_block.terminator = match &callee_block.terminator {
            Terminator::Ret(val) => {
                // Return → jump to merge block
                // If there's a return value and the call has a dest, create a temp var
                // that we'll feed into a Phi in the merge block
                if let Some(val) = val {
                    if call_dest.is_some() {
                        let remapped_val = remap_operand(val, var_offset);
                        let temp_var = match &remapped_val {
                            Operand::Var(v) => *v,
                            _ => {
                                // Need to materialize constant into a temp var
                                let tv = VarId(next_temp_var);
                                next_temp_var += 1;
                                new_block.instructions.push(Instruction::Copy {
                                    dest: tv,
                                    src: remapped_val,
                                });
                                tv
                            }
                        };
                        ret_sites.push((new_block.id, temp_var));
                    }
                }
                Terminator::Br(merge_block_id)
            }
            Terminator::Br(target) => Terminator::Br(BlockId(target.0 + block_offset)),
            Terminator::CondBr { cond, then_block, else_block } => Terminator::CondBr {
                cond: remap_operand(cond, var_offset),
                then_block: BlockId(then_block.0 + block_offset),
                else_block: BlockId(else_block.0 + block_offset),
            },
            Terminator::Unreachable => Terminator::Unreachable,
        };

        inlined_blocks.push(new_block);
    }

    // Create the merge block (continuation after inline)
    let mut merge_instructions = Vec::new();

    // Add Phi node for return value if needed
    if let Some(dest) = call_dest {
        if ret_sites.len() == 1 {
            // Single return site: simple Copy (no Phi needed)
            merge_instructions.push(Instruction::Copy {
                dest,
                src: Operand::Var(ret_sites[0].1),
            });
        } else if ret_sites.len() > 1 {
            // Multiple return sites: Phi node
            merge_instructions.push(Instruction::Phi {
                dest,
                preds: ret_sites.iter().map(|(bid, vid)| (*bid, *vid)).collect(),
            });
        }
    }

    // Append post-call instructions after the Phi/Copy
    merge_instructions.extend(post_call_insts);

    let merge_block = BasicBlock {
        id: merge_block_id,
        instructions: merge_instructions,
        terminator: orig_terminator,
        is_label_target: false,
    };

    // Copy callee's var_types into caller with remapped VarIds
    for (var, ty) in &callee.var_types {
        caller.var_types.insert(VarId(var.0 + var_offset), ty.clone());
    }

    // Append all new blocks
    for block in inlined_blocks {
        caller.blocks.push(block);
    }
    caller.blocks.push(merge_block);

    // Fix Phi nodes in successor blocks: the merge_block now owns the
    // original terminator, so any block that was a successor of orig_block
    // via that terminator now has merge_block as a predecessor instead.
    // We need to update Phi preds that reference orig_block_id → merge_block_id.
    let successor_ids: Vec<BlockId> = match &caller.blocks.last().unwrap().terminator {
        Terminator::Br(t) => vec![*t],
        Terminator::CondBr { then_block, else_block, .. } => vec![*then_block, *else_block],
        _ => vec![],
    };
    for block in &mut caller.blocks {
        if successor_ids.contains(&block.id) {
            for inst in &mut block.instructions {
                if let Instruction::Phi { preds, .. } = inst {
                    for (bid, _) in preds.iter_mut() {
                        if *bid == orig_block_id {
                            *bid = merge_block_id;
                        }
                    }
                }
            }
        }
    }
}

fn remap_operand(op: &Operand, var_offset: usize) -> Operand {
    match op {
        Operand::Var(v) => Operand::Var(VarId(v.0 + var_offset)),
        other => other.clone(),
    }
}

fn remap_instruction(inst: &Instruction, var_offset: usize, block_offset: usize) -> Instruction {
    match inst {
        Instruction::Binary { dest, op, left, right } => Instruction::Binary {
            dest: VarId(dest.0 + var_offset),
            op: op.clone(),
            left: remap_operand(left, var_offset),
            right: remap_operand(right, var_offset),
        },
        Instruction::FloatBinary { dest, op, left, right } => Instruction::FloatBinary {
            dest: VarId(dest.0 + var_offset),
            op: op.clone(),
            left: remap_operand(left, var_offset),
            right: remap_operand(right, var_offset),
        },
        Instruction::Unary { dest, op, src } => Instruction::Unary {
            dest: VarId(dest.0 + var_offset),
            op: op.clone(),
            src: remap_operand(src, var_offset),
        },
        Instruction::FloatUnary { dest, op, src } => Instruction::FloatUnary {
            dest: VarId(dest.0 + var_offset),
            op: op.clone(),
            src: remap_operand(src, var_offset),
        },
        Instruction::Phi { dest, preds } => Instruction::Phi {
            dest: VarId(dest.0 + var_offset),
            preds: preds.iter().map(|(b, v)| (BlockId(b.0 + block_offset), VarId(v.0 + var_offset))).collect(),
        },
        Instruction::Copy { dest, src } => Instruction::Copy {
            dest: VarId(dest.0 + var_offset),
            src: remap_operand(src, var_offset),
        },
        Instruction::Cast { dest, src, r#type } => Instruction::Cast {
            dest: VarId(dest.0 + var_offset),
            src: remap_operand(src, var_offset),
            r#type: r#type.clone(),
        },
        Instruction::Alloca { dest, r#type } => Instruction::Alloca {
            dest: VarId(dest.0 + var_offset),
            r#type: r#type.clone(),
        },
        Instruction::Load { dest, addr, value_type, volatile } => Instruction::Load {
            dest: VarId(dest.0 + var_offset),
            addr: remap_operand(addr, var_offset),
            value_type: value_type.clone(),
            volatile: *volatile,
        },
        Instruction::Store { addr, src, value_type, volatile } => Instruction::Store {
            addr: remap_operand(addr, var_offset),
            src: remap_operand(src, var_offset),
            value_type: value_type.clone(),
            volatile: *volatile,
        },
        Instruction::GetElementPtr { dest, base, index, element_type } => Instruction::GetElementPtr {
            dest: VarId(dest.0 + var_offset),
            base: remap_operand(base, var_offset),
            index: remap_operand(index, var_offset),
            element_type: element_type.clone(),
        },
        Instruction::Call { dest, name, args } => Instruction::Call {
            dest: dest.map(|d| VarId(d.0 + var_offset)),
            name: name.clone(),
            args: args.iter().map(|a| remap_operand(a, var_offset)).collect(),
        },
        Instruction::IndirectCall { dest, func_ptr, args } => Instruction::IndirectCall {
            dest: dest.map(|d| VarId(d.0 + var_offset)),
            func_ptr: remap_operand(func_ptr, var_offset),
            args: args.iter().map(|a| remap_operand(a, var_offset)).collect(),
        },
        Instruction::VaStart { list, arg_index } => Instruction::VaStart {
            list: remap_operand(list, var_offset),
            arg_index: *arg_index,
        },
        Instruction::VaEnd { list } => Instruction::VaEnd {
            list: remap_operand(list, var_offset),
        },
        Instruction::VaCopy { dest, src } => Instruction::VaCopy {
            dest: remap_operand(dest, var_offset),
            src: remap_operand(src, var_offset),
        },
        Instruction::VaArg { dest, list, r#type } => Instruction::VaArg {
            dest: VarId(dest.0 + var_offset),
            list: remap_operand(list, var_offset),
            r#type: r#type.clone(),
        },
        Instruction::InlineAsm { template, outputs, inputs, output_constraints, input_constraints, clobbers, is_volatile } => {
            Instruction::InlineAsm {
                template: template.clone(),
                outputs: outputs.iter().map(|v| VarId(v.0 + var_offset)).collect(),
                inputs: inputs.iter().map(|o| remap_operand(o, var_offset)).collect(),
                output_constraints: output_constraints.clone(),
                input_constraints: input_constraints.clone(),
                clobbers: clobbers.clone(),
                is_volatile: *is_volatile,
            }
        }
        Instruction::Simd { op, dest, operands, elem_type, width } => Instruction::Simd {
            op: op.clone(),
            dest: dest.map(|d| VarId(d.0 + var_offset)),
            operands: operands.iter().map(|o| remap_operand(o, var_offset)).collect(),
            elem_type: elem_type.clone(),
            width: *width,
        },
    }
}

fn find_max_var(func: &Function) -> usize {
    let mut max = 0;
    // Check params
    for (_, v) in &func.params {
        max = max.max(v.0);
    }
    // Check var_types
    for (v, _) in &func.var_types {
        max = max.max(v.0);
    }
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Some(dest) = inst.dest() {
                max = max.max(dest.0);
            }
            // Also check operands for Var references
            match inst {
                Instruction::Binary { left, right, .. } |
                Instruction::FloatBinary { left, right, .. } => {
                    if let Operand::Var(v) = left { max = max.max(v.0); }
                    if let Operand::Var(v) = right { max = max.max(v.0); }
                }
                Instruction::Copy { src, .. } |
                Instruction::Cast { src, .. } |
                Instruction::Unary { src, .. } |
                Instruction::FloatUnary { src, .. } => {
                    if let Operand::Var(v) = src { max = max.max(v.0); }
                }
                Instruction::Load { addr, .. } => {
                    if let Operand::Var(v) = addr { max = max.max(v.0); }
                }
                Instruction::Store { addr, src, .. } => {
                    if let Operand::Var(v) = addr { max = max.max(v.0); }
                    if let Operand::Var(v) = src { max = max.max(v.0); }
                }
                Instruction::GetElementPtr { base, index, .. } => {
                    if let Operand::Var(v) = base { max = max.max(v.0); }
                    if let Operand::Var(v) = index { max = max.max(v.0); }
                }
                Instruction::Call { args, .. } => {
                    for a in args {
                        if let Operand::Var(v) = a { max = max.max(v.0); }
                    }
                }
                Instruction::IndirectCall { func_ptr, args, .. } => {
                    if let Operand::Var(v) = func_ptr { max = max.max(v.0); }
                    for a in args {
                        if let Operand::Var(v) = a { max = max.max(v.0); }
                    }
                }
                Instruction::Phi { preds, .. } => {
                    for (_, v) in preds { max = max.max(v.0); }
                }
                _ => {}
            }
        }
        // Check terminator operands
        match &block.terminator {
            Terminator::Ret(Some(Operand::Var(v))) => { max = max.max(v.0); }
            Terminator::CondBr { cond: Operand::Var(v), .. } => { max = max.max(v.0); }
            _ => {}
        }
    }
    max
}
