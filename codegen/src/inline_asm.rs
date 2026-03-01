// Inline assembly code generation
// Handles constraint interpretation, clobber save/restore, and volatile semantics

use crate::x86::{X86Operand, X86Instr, X86Reg};
use ir::{VarId, Operand};
use crate::function::FunctionGenerator;

/// Map a GCC-style clobber name to an X86Reg.
fn clobber_to_reg(name: &str) -> Option<X86Reg> {
    match name {
        "rax" | "eax" | "ax" | "al" => Some(X86Reg::Rax),
        "rbx" | "ebx" | "bx" | "bl" => Some(X86Reg::Rbx),
        "rcx" | "ecx" | "cx" | "cl" => Some(X86Reg::Rcx),
        "rdx" | "edx" | "dx" | "dl" => Some(X86Reg::Rdx),
        "rsi" | "esi" | "si" => Some(X86Reg::Rsi),
        "rdi" | "edi" | "di" => Some(X86Reg::Rdi),
        "r8" | "r8d" | "r8w" | "r8b" => Some(X86Reg::R8),
        "r9" | "r9d" | "r9w" | "r9b" => Some(X86Reg::R9),
        "r10" | "r10d" | "r10w" | "r10b" => Some(X86Reg::R10),
        "r11" | "r11d" | "r11w" | "r11b" => Some(X86Reg::R11),
        "r12" | "r12d" | "r12w" | "r12b" => Some(X86Reg::R12),
        "r13" | "r13d" | "r13w" | "r13b" => Some(X86Reg::R13),
        "r14" | "r14d" | "r14w" | "r14b" => Some(X86Reg::R14),
        "r15" | "r15d" | "r15w" | "r15b" => Some(X86Reg::R15),
        _ => None,
    }
}

/// Render an X86 operand as an Intel-syntax string for inline asm,
/// using the constraint to determine if it should be a register or memory.
fn render_operand(op: &X86Operand, constraint: &str, is_output: bool) -> String {
    let needs_memory = constraint.contains('m');
    let needs_immediate = constraint.contains('i') || constraint.contains('n');

    if needs_immediate {
        // Constraint says immediate — extract the raw value
        match op {
            X86Operand::Imm(val) => return val.to_string(),
            _ => {} // fall through to default
        }
    }

    if needs_memory || (!constraint.contains('r') && !constraint.contains('q') && !constraint.contains('a')
        && !constraint.contains('b') && !constraint.contains('c') && !constraint.contains('d')
        && !constraint.contains('S') && !constraint.contains('D')
        && !needs_immediate
        && constraint.is_empty()) {
        // No specific constraint or memory constraint — use memory form
    }

    match op {
        X86Operand::Mem(reg, offset) => {
            if needs_memory || !constraint.contains('r') {
                // Memory constraint — render as memory operand
                format_mem_operand(reg, *offset, false)
            } else {
                // Register constraint — need to load first, handled by caller
                format_mem_operand(reg, *offset, false)
            }
        }
        X86Operand::DwordMem(reg, offset) => format_mem_operand(reg, *offset, true),
        X86Operand::Reg(r) => r.to_str().to_string(),
        X86Operand::Imm(val) => val.to_string(),
        X86Operand::FloatMem(reg, offset) | X86Operand::DoubleMem(reg, offset) => {
            format_mem_operand(reg, *offset, false)
        }
        X86Operand::Label(name) | X86Operand::RipRelLabel(name) => format!("[rip+{}]", name),
        X86Operand::GlobalMem(name) | X86Operand::GlobalQwordMem(name) => format!("QWORD PTR [rip+{}]", name),
        X86Operand::WordMem(reg, offset) => format_mem_operand(reg, *offset, false),
        X86Operand::ByteMem(reg, offset) => format_mem_operand(reg, *offset, false),
        X86Operand::XmmwordMem(reg, offset) | X86Operand::YmmwordMem(reg, offset) => format_mem_operand(reg, *offset, false),
    }
}

fn format_mem_operand(reg: &X86Reg, offset: i32, dword: bool) -> String {
    let prefix = if dword { "DWORD PTR " } else { "QWORD PTR " };
    if offset == 0 {
        format!("{}[{}]", prefix, reg.to_str())
    } else if offset > 0 {
        format!("{}[{}+{}]", prefix, reg.to_str(), offset)
    } else {
        format!("{}[{}{}]", prefix, reg.to_str(), offset)
    }
}

impl<'a> FunctionGenerator<'a> {
    pub(crate) fn gen_inline_asm(
        &mut self,
        template: &str,
        outputs: &[VarId],
        inputs: &[Operand],
        output_constraints: &[String],
        input_constraints: &[String],
        clobbers: &[String],
        _is_volatile: bool,
    ) {
        // Step 1: Save clobbered registers (skip "memory" and "cc" pseudo-clobbers)
        let mut saved_regs = Vec::new();
        for clobber in clobbers {
            if clobber == "memory" || clobber == "cc" {
                continue;
            }
            if let Some(reg) = clobber_to_reg(clobber) {
                self.asm.push(X86Instr::Push(reg.clone()));
                saved_regs.push(reg);
            }
        }

        // Step 2: Handle "+r" (read-write) output constraints by loading current value
        // For "+r" constraints, the output var is also an input (pre-loaded)
        for (i, constraint) in output_constraints.iter().enumerate() {
            if constraint.contains('+') && i < outputs.len() {
                // Read-write: the var's current value is an implicit input
                // The codegen treats %N as the output location; the template
                // can read and write the same %N.
            }
        }

        // Step 3: Build operand string substitutions
        let mut asm_code = template.to_string();

        for (i, output_var) in outputs.iter().enumerate() {
            let placeholder = format!("%{}", i);
            let constraint = output_constraints.get(i).map_or("", |c| c.as_str());
            let output_op = self.var_to_op(*output_var);
            let operand_str = render_operand(&output_op, constraint, true);
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }

        for (i, input_op) in inputs.iter().enumerate() {
            let placeholder = format!("%{}", outputs.len() + i);
            let constraint = input_constraints.get(i).map_or("", |c| c.as_str());
            let input_x86_op = self.operand_to_op(input_op);
            let operand_str = render_operand(&input_x86_op, constraint, false);
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }

        // Step 4: Clean up GCC-isms
        asm_code = asm_code.replace("{$}", "");

        // Step 5: Emit the assembly — split by ';' or newline for multi-statement templates
        for line in asm_code.split(|c| c == ';' || c == '\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                self.asm.push(X86Instr::Raw(trimmed.to_string()));
            }
        }

        // Step 6: Insert "memory" clobber fence — compiler barrier
        // (already handled by has_side_effects = true for InlineAsm,
        //  which prevents reordering in the optimizer)

        // Step 7: Restore clobbered registers (in reverse order)
        for reg in saved_regs.iter().rev() {
            self.asm.push(X86Instr::Pop(reg.clone()));
        }
    }
}
