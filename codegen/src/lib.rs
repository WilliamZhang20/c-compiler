mod x86;
mod regalloc;
mod peephole;
mod types;
mod instructions;
mod function;
mod float_ops;
mod memory_ops;
mod call_ops;

use model::Type;
use ir::IRProgram;
use std::collections::HashMap;

pub use x86::{X86Reg, X86Operand, X86Instr, emit_asm};
pub use regalloc::{PhysicalReg, allocate_registers};
use peephole::apply_peephole;
use function::FunctionGenerator;

pub struct Codegen {
    // Shared state
    structs: HashMap<String, model::StructDef>,
    unions: HashMap<String, model::UnionDef>,
    float_constants: HashMap<String, f64>,
    next_float_const: usize,
    func_return_types: HashMap<String, Type>,
    enable_regalloc: bool,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            unions: HashMap::new(),
            float_constants: HashMap::new(),
            next_float_const: 0,
            func_return_types: HashMap::new(),
            enable_regalloc: true,
        }
    }

    pub fn gen_program(&mut self, prog: &IRProgram) -> String {
        // Debug output removed
        
        self.structs.clear();
        self.unions.clear();
        for s_def in &prog.structs {
            self.structs.insert(s_def.name.clone(), s_def.clone());
        }
        for u_def in &prog.unions {
            self.unions.insert(u_def.name.clone(), u_def.clone());
        }
        self.float_constants.clear();
        self.next_float_const = 0;
        
        // Build function signature map for return type inference in calls
        self.func_return_types.clear();
        for func in &prog.functions {
            self.func_return_types.insert(func.name.clone(), func.return_type.clone());
        }
        
        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n");
        
        // Strings
        if !prog.global_strings.is_empty() {
            output.push_str(".data\n");
            for (label, content) in &prog.global_strings {
                // Properly escape string for assembly output
                let escaped = content
                    .replace("\\", "\\\\")  // Backslash must be first
                    .replace("\n", "\\n")   // Newline
                    .replace("\r", "\\r")   // Carriage return
                    .replace("\t", "\\t")   // Tab
                    .replace("\"", "\\\"")  // Double quote
                    .replace("\0", "\\0");  // Null (though .asciz adds one)
                output.push_str(&format!("{}: .asciz \"{}\"\n", label, escaped));
            }
        }

        // Globals (use .quad for all to simplify codegen - int values will be sign-extended)
        if !prog.globals.is_empty() {
             if prog.global_strings.is_empty() { output.push_str(".data\n"); }
             for g in &prog.globals {
                 // Handle section attribute
                 let mut in_custom_section = false;
                 for attr in &g.attributes {
                     if let model::Attribute::Section(section_name) = attr {
                         output.push_str(&format!(".section {}\n", section_name));
                         in_custom_section = true;
                     }
                 }
                 
                 output.push_str(&format!(".globl {}\n", g.name));
                 
                 // Handle aligned attribute (default to 4-byte alignment otherwise)
                 let mut alignment = 4;
                 for attr in &g.attributes {
                     if let model::Attribute::Aligned(n) = attr {
                         alignment = *n;
                     }
                 }
                 output.push_str(&format!(".align {}\n", alignment));
                 
                 if let Some(init) = &g.init {
                     // Extract string from Expr
                     let init_str = match init {
                         model::Expr::Constant(c) => c.to_string(),
                         model::Expr::FloatConstant(f) => format!("{:.15}", f),
                         _ => "0".to_string(),
                     };
                     
                     // Initialized - use .quad for all for simplicity
                     match &g.r#type {
                         Type::Char | Type::UnsignedChar => output.push_str(&format!("{}: .byte {}\n", g.name, init_str)),
                         Type::Int | Type::UnsignedInt => output.push_str(&format!("{}: .long {}\n", g.name, init_str)),
                         _ => output.push_str(&format!("{}: .quad {}\n", g.name, init_str)),
                     }
                 } else {
                     // Uninitialized
                     output.push_str(&format!("{}: .long 0\n", g.name));
                 }
                 
                 // Switch back to .data section if we were in a custom section
                 if in_custom_section {
                     output.push_str(".data\n");
                 }
             }
        }

        output.push_str(".text\n");
        
        for func in &prog.functions {
            // Emit .globl directive for all functions (for linking)
            output.push_str(&format!(".globl {}\n", func.name));
            
            let func_gen = FunctionGenerator::new(
                &self.structs,
                &self.unions,
                &self.func_return_types,
                &mut self.float_constants,
                &mut self.next_float_const,
                self.enable_regalloc,
            );
            
            let mut func_asm = func_gen.gen_function(func);
            
            // Apply peephole optimizations
            apply_peephole(&mut func_asm);
            
            output.push_str(&emit_asm(&func_asm));
        }
        
        // Emit float constants in .data section
        if !self.float_constants.is_empty() {
            output.push_str("\n.data\n");
            output.push_str(".align 16\n");
            let mut sorted_consts: Vec<_> = self.float_constants.iter().collect();
            sorted_consts.sort_by_key(|(label, _)| label.as_str());
            for (label, value) in sorted_consts {
                // Convert f64 to f32 for single-precision floats
                let f32_value = *value as f32;
                let bits = f32_value.to_bits();
                output.push_str(&format!("{}: .long 0x{:08x}\n", label, bits));
            }
        }
        
        output
    }
}
