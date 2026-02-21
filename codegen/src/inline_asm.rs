// Inline assembly code generation
// Extracted from function.rs: gen_inline_asm

use crate::x86::{X86Operand, X86Instr};
use ir::{VarId, Operand};
use crate::function::FunctionGenerator;

impl<'a> FunctionGenerator<'a> {
    pub(crate) fn gen_inline_asm(&mut self, template: &str, outputs: &[VarId], inputs: &[Operand], _clobbers: &[String], _is_volatile: bool) {
        let mut asm_code = template.to_string();
        for (i, output_var) in outputs.iter().enumerate() {
            let placeholder = format!("%{}", i);
            let output_op = self.var_to_op(*output_var);
            let operand_str = match &output_op {
                X86Operand::Mem(reg, offset) => {
                    if *offset == 0 { format!("DWORD PTR [{}]", reg.to_str()) } 
                    else { format!("DWORD PTR [{}{}]", reg.to_str(), if *offset > 0 { format!("+{}", offset) } else { offset.to_string() }) }
                }
                X86Operand::Reg(r) => r.to_str().to_string(),
                _ => "eax".to_string(),
            };
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        for (i, input_op) in inputs.iter().enumerate() {
            let placeholder = format!("%{}", outputs.len() + i);
            let input_x86_op = self.operand_to_op(input_op);
            let operand_str = match &input_x86_op {
                X86Operand::Imm(val) => val.to_string(),
                X86Operand::Reg(r) => r.to_str().to_string(),
                X86Operand::Mem(reg, offset) => {
                    if *offset == 0 { format!("DWORD PTR [{}]", reg.to_str()) } 
                    else { format!("DWORD PTR [{}{}]", reg.to_str(), if *offset > 0 { format!("+{}", offset) } else { offset.to_string() }) }
                }
                _ => "0".to_string(),
            };
            asm_code = asm_code.replace(&placeholder, &operand_str);
        }
        asm_code = asm_code.replace("{$}", "");
        self.asm.push(X86Instr::Raw(asm_code));
    }
}
