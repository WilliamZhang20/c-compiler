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
    float_constants: HashMap<String, (f64, bool)>,
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

    /// Test helper: insert a struct definition for unit tests.
    #[cfg(test)]
    pub(crate) fn add_struct(&mut self, s_def: model::StructDef) {
        self.structs.insert(s_def.name.clone(), s_def);
    }

    /// Test helper: insert a union definition for unit tests.
    #[cfg(test)]
    pub(crate) fn add_union(&mut self, u_def: model::UnionDef) {
        self.unions.insert(u_def.name.clone(), u_def);
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
        
        // ── Pre-classify globals into sections ──────────────────
        // One pass instead of repeated filter scans.
        let mut rodata_globals: Vec<&model::GlobalVar> = Vec::new();
        let mut data_globals: Vec<&model::GlobalVar> = Vec::new();
        let mut bss_globals: Vec<&model::GlobalVar> = Vec::new();
        let mut custom_globals: Vec<(&model::GlobalVar, String)> = Vec::new();

        for g in &prog.globals {
            // Skip extern declarations with no initializer
            if g.is_extern && g.init.is_none() { continue; }

            // Custom section overrides everything else
            if let Some(section_name) = g.attributes.iter().find_map(|a| {
                if let model::Attribute::Section(name) = a { Some(name.clone()) } else { None }
            }) {
                custom_globals.push((g, section_name));
                continue;
            }

            // Const with initializer → rodata
            if g.qualifiers.is_const && g.init.is_some() {
                rodata_globals.push(g);
                continue;
            }

            // Classify remaining by initializer
            match &g.init {
                Some(init) if !Self::is_zero_init(init) => data_globals.push(g),
                _ => bss_globals.push(g), // None or zero-init
            }
        }

        let mut output = String::new();
        output.push_str(".intel_syntax noprefix\n");
        
        // ── .rodata section ─────────────────────────────────────
        if !prog.global_strings.is_empty() || !rodata_globals.is_empty() {
            output.push_str(".section .rodata\n");
            
            // String constants
            for (label, content) in &prog.global_strings {
                let escaped = content
                    .replace("\\", "\\\\")
                    .replace("\n", "\\n")
                    .replace("\r", "\\r")
                    .replace("\t", "\\t")
                    .replace("\"", "\\\"")
                    .replace("\0", "\\0");
                output.push_str(&format!("{}: .asciz \"{}\"\n", label, escaped));
            }
            
            for g in &rodata_globals {
                self.emit_global_var(&mut output, g);
            }
        }

        // ── .data section ───────────────────────────────────────
        if !data_globals.is_empty() {
            output.push_str(".data\n");
            for g in &data_globals {
                self.emit_global_var(&mut output, g);
            }
        }
        
        // ── .bss section ────────────────────────────────────────
        if !bss_globals.is_empty() {
            output.push_str(".bss\n");
            for g in &bss_globals {
                if g.is_static {
                    // Static linkage
                } else {
                    output.push_str(&format!(".globl {}\n", g.name));
                }
                
                let mut alignment = 4;
                for attr in &g.attributes {
                    if let model::Attribute::Aligned(n) = attr {
                        alignment = *n;
                    }
                }
                output.push_str(&format!(".align {}\n", alignment));
                
                let size = self.global_var_size(g);
                output.push_str(&format!("{}:\n", g.name));
                output.push_str(&format!("    .zero {}\n", size));
            }
        }

        // ── Custom-section globals ──────────────────────────────
        for (g, section_name) in &custom_globals {
            match self.target.platform {
                model::Platform::Linux => {
                    output.push_str(&format!(".section {}, \"aw\", @progbits\n", section_name));
                }
                model::Platform::Windows => {
                    output.push_str(&format!(".section {}\n", section_name));
                }
            }
            self.emit_global_var(&mut output, g);
        }

        output.push_str(".text\n");
        
        for func in &prog.functions {
            // Emit visibility directive
            if func.is_static {
                // Static linkage: internal visibility only
            } else {
                output.push_str(&format!(".globl {}\n", func.name));
            }
            
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
        
        // Emit float constants in .rodata section
        if !self.float_constants.is_empty() {
            output.push_str("\n.section .rodata\n");
            output.push_str(".align 16\n");
            let mut sorted_consts: Vec<_> = self.float_constants.iter().collect();
            sorted_consts.sort_by_key(|(label, _)| label.as_str());
            for (label, (value, is_double)) in sorted_consts {
                if *is_double {
                    let bits = value.to_bits();
                    output.push_str(&format!("{}: .quad 0x{:016x}\n", label, bits));
                } else {
                    let f32_value = *value as f32;
                    let bits = f32_value.to_bits();
                    output.push_str(&format!("{}: .long 0x{:08x}\n", label, bits));
                }
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
    
    /// Emit a single global variable (label + data directives).
    /// Used by .rodata, .data, and custom section emission.
    fn emit_global_var(&self, output: &mut String, g: &model::GlobalVar) {
        if g.is_static {
            // Static linkage: not visible outside this translation unit
        } else {
            output.push_str(&format!(".globl {}\n", g.name));
        }
        
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
                    output.push_str(&format!("{}:\n", g.name));
                    self.emit_init_list_data(output, &g.r#type, items);
                }
                model::Expr::StringLiteral(s) => {
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
            // Uninitialized (should only happen in .bss path, but handle for safety)
            let size = self.type_size(&g.r#type);
            output.push_str(&format!("{}: .zero {}\n", g.name, size));
        }
    }
    
    /// Check if an initializer expression is all-zeros.
    fn is_zero_init(init: &model::Expr) -> bool {
        match init {
            model::Expr::Constant(0) => true,
            model::Expr::FloatConstant(f) => f.to_bits() == 0,
            model::Expr::InitList(items) => {
                items.iter().all(|item| Self::is_zero_init(&item.value))
            }
            _ => false,
        }
    }
    
    /// Get the total size in bytes of a global variable.
    fn global_var_size(&self, g: &model::GlobalVar) -> usize {
        self.type_size(&g.r#type)
    }
}
