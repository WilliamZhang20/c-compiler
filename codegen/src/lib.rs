mod x86;
mod regalloc;
mod peephole;
mod types;
mod instructions;
mod function;
mod float_ops;
mod memory_ops;
mod call_ops;
mod calling_convention;
mod control_flow;
mod inline_asm;
mod liveness;
mod globals;

use model::Type;
use ir::IRProgram;
use std::collections::HashMap;

pub use x86::{X86Reg, X86Operand, X86Instr, emit_asm};
pub use regalloc::{PhysicalReg, allocate_registers};
use peephole::apply_peephole;
use function::FunctionGenerator;
pub use model::TargetConfig;

pub struct Codegen {
    // Shared state
    structs: HashMap<String, model::StructDef>,
    unions: HashMap<String, model::UnionDef>,
    float_constants: HashMap<String, f64>,
    next_float_const: usize,
    func_return_types: HashMap<String, Type>,
    enable_regalloc: bool,
    target: TargetConfig,
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
            target: TargetConfig::host(),
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
                         // Generate platform-specific section directive
                         match self.target.platform {
                             model::Platform::Linux => {
                                 // ELF format: section name, flags, type
                                 // "aw" = allocatable, writable; @progbits = contains data
                                 output.push_str(&format!(".section {}, \"aw\", @progbits\n", section_name));
                             }
                             model::Platform::Windows => {
                                 // PE/COFF format: section name only (no @type syntax)
                                 output.push_str(&format!(".section {}\n", section_name));
                             }
                         }
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
                     match init {
                         model::Expr::InitList(items) => {
                             // Emit label, then each element
                             output.push_str(&format!("{}:\n", g.name));
                             self.emit_init_list_data(&mut output, &g.r#type, items);
                         }
                         model::Expr::StringLiteral(s) => {
                             // Global string array: emit as .asciz
                             let escaped = s
                                 .replace("\\", "\\\\")
                                 .replace("\n", "\\n")
                                 .replace("\r", "\\r")
                                 .replace("\t", "\\t")
                                 .replace("\"", "\\\"")
                                 .replace("\0", "\\0");
                             output.push_str(&format!("{}: .asciz \"{}\"\n", g.name, escaped));
                         }
                         _ => {
                             // Scalar init
                             let init_str = match init {
                                 model::Expr::Constant(c) => c.to_string(),
                                 model::Expr::FloatConstant(f) => format!("{:.15}", f),
                                 _ => "0".to_string(),
                             };
                             match &g.r#type {
                                 Type::Char | Type::UnsignedChar => output.push_str(&format!("{}: .byte {}\n", g.name, init_str)),
                                 Type::Int | Type::UnsignedInt => output.push_str(&format!("{}: .long {}\n", g.name, init_str)),
                                 _ => output.push_str(&format!("{}: .quad {}\n", g.name, init_str)),
                             }
                         }
                     }
                 } else {
                     // Uninitialized
                     match &g.r#type {
                         Type::Array(inner, size) => {
                             let elem_bytes: usize = match inner.as_ref() {
                                 Type::Char | Type::UnsignedChar => 1,
                                 Type::Short | Type::UnsignedShort => 2,
                                 Type::Int | Type::UnsignedInt | Type::Float => 4,
                                 Type::Typedef(n) => match n.as_str() {
                                     "int8_t" | "uint8_t" | "int8" => 1,
                                     "int16_t" | "uint16_t" => 2,
                                     "int32_t" | "uint32_t" => 4,
                                     "int64_t" | "uint64_t" | "size_t" => 8,
                                     _ => 4,
                                 },
                                 _ => 8,
                             };
                             output.push_str(&format!("{}: .zero {}\n", g.name, elem_bytes * size));
                         }
                         _ => output.push_str(&format!("{}: .long 0\n", g.name)),
                     }
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
            
            // Check for weak attribute
            if func.attributes.iter().any(|a| matches!(a, model::Attribute::Weak)) {
                output.push_str(&format!(".weak {}\n", func.name));
            }
            
            // Check for section attribute on functions
            let mut func_in_custom_section = false;
            for attr in &func.attributes {
                if let model::Attribute::Section(section_name) = attr {
                    output.push_str(&format!(".section {}, \"ax\", @progbits\n", section_name));
                    func_in_custom_section = true;
                }
            }
            
            let func_gen = FunctionGenerator::new(
                &self.structs,
                &self.unions,
                &self.func_return_types,
                &mut self.float_constants,
                &mut self.next_float_const,
                self.enable_regalloc,
                &self.target,
            );
            
            let mut func_asm = func_gen.gen_function(func);
            
            // Apply peephole optimizations
            apply_peephole(&mut func_asm);
            
            output.push_str(&emit_asm(&func_asm));
            
            // Switch back to .text if we were in a custom section
            if func_in_custom_section {
                output.push_str(".text\n");
            }
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
        
        // Emit .init_array / .fini_array entries for constructor/destructor functions
        for func in &prog.functions {
            if func.attributes.iter().any(|a| matches!(a, model::Attribute::Constructor)) {
                output.push_str(&format!("\n.section .init_array,\"aw\",@init_array\n"));
                output.push_str(".align 8\n");
                output.push_str(&format!(".quad {}\n", func.name));
            }
            if func.attributes.iter().any(|a| matches!(a, model::Attribute::Destructor)) {
                output.push_str(&format!("\n.section .fini_array,\"aw\",@fini_array\n"));
                output.push_str(".align 8\n");
                output.push_str(&format!(".quad {}\n", func.name));
            }
        }
        
        // Add .note.GNU-stack section for Linux to mark stack as non-executable
        if matches!(self.target.platform, model::Platform::Linux) {
            output.push_str("\n.section .note.GNU-stack,\"\",@progbits\n");
        }
        
        output
    }

}
