// Global data emission helpers for codegen
// Extracted from lib.rs: emit_init_list_data, find_init_item, emit_scalar_data,
// emit_zero_data, type_size, type_alignment, struct_size

use model::Type;
use crate::Codegen;

impl Codegen {
    /// Emit assembly data directives for an initializer list.
    pub(crate) fn emit_init_list_data(&self, output: &mut String, ty: &Type, items: &[model::InitItem]) {
        match ty {
            Type::Array(inner, size) => {
                // Emit each element, filling remaining with zeros
                for i in 0..*size {
                    if let Some(item) = self.find_init_item(items, i) {
                        match &item.value {
                            model::Expr::InitList(nested) => {
                                self.emit_init_list_data(output, inner, nested);
                            }
                            model::Expr::Constant(c) => {
                                self.emit_scalar_data(output, inner, *c);
                            }
                            model::Expr::FloatConstant(f) => {
                                let f32_val = *f as f32;
                                output.push_str(&format!("    .long 0x{:08x}\n", f32_val.to_bits()));
                            }
                            _ => {
                                self.emit_zero_data(output, inner);
                            }
                        }
                    } else {
                        self.emit_zero_data(output, inner);
                    }
                }
            }
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let s_def = s_def.clone();
                    let is_packed = s_def.attributes.iter().any(|a| matches!(a, model::Attribute::Packed));
                    let mut current_offset: usize = 0;
                    let mut field_idx = 0usize;

                    for item in items {
                        let target_idx = match &item.designator {
                            Some(model::Designator::Field(fname)) => {
                                s_def.fields.iter().position(|f| &f.name == fname).unwrap_or(field_idx)
                            }
                            _ => field_idx,
                        };

                        // Compute offset of target field, emit padding if needed
                        let mut offset: usize = 0;
                        for fi in 0..=target_idx {
                            if !is_packed {
                                let align = self.type_alignment(&s_def.fields[fi].field_type);
                                offset = (offset + align - 1) / align * align;
                            }
                            if fi < target_idx {
                                offset += self.type_size(&s_def.fields[fi].field_type);
                            }
                        }
                        if offset > current_offset {
                            output.push_str(&format!("    .zero {}\n", offset - current_offset));
                        }

                        let field = &s_def.fields[target_idx];
                        match &item.value {
                            model::Expr::InitList(nested) => {
                                self.emit_init_list_data(output, &field.field_type, nested);
                            }
                            model::Expr::Constant(c) => {
                                self.emit_scalar_data(output, &field.field_type, *c);
                            }
                            model::Expr::FloatConstant(f) => {
                                let f32_val = *f as f32;
                                output.push_str(&format!("    .long 0x{:08x}\n", f32_val.to_bits()));
                            }
                            _ => {
                                self.emit_zero_data(output, &field.field_type);
                            }
                        }
                        current_offset = offset + self.type_size(&field.field_type);
                        field_idx = target_idx + 1;
                    }

                    // Emit trailing padding
                    let struct_size = self.struct_size(&s_def, is_packed);
                    if current_offset < struct_size {
                        output.push_str(&format!("    .zero {}\n", struct_size - current_offset));
                    }
                }
            }
            _ => {
                // Scalar type with init list (unusual but valid for single-element)
                if let Some(item) = items.first() {
                    if let model::Expr::Constant(c) = &item.value {
                        self.emit_scalar_data(output, ty, *c);
                    } else {
                        self.emit_zero_data(output, ty);
                    }
                }
            }
        }
    }

    /// Find an init item for a given positional index (handles designated initializers).
    pub(crate) fn find_init_item<'b>(&self, items: &'b [model::InitItem], index: usize) -> Option<&'b model::InitItem> {
        // Check designated first
        for item in items {
            if let Some(model::Designator::Index(idx)) = &item.designator {
                if *idx as usize == index {
                    return Some(item);
                }
            }
        }
        // Otherwise use positional
        let mut pos = 0usize;
        for item in items {
            if item.designator.is_none() {
                if pos == index {
                    return Some(item);
                }
                pos += 1;
            }
        }
        None
    }

    /// Emit a scalar data directive for a given type.
    pub(crate) fn emit_scalar_data(&self, output: &mut String, ty: &Type, value: i64) {
        match ty {
            Type::Char | Type::UnsignedChar => output.push_str(&format!("    .byte {}\n", value)),
            Type::Short | Type::UnsignedShort => output.push_str(&format!("    .short {}\n", value)),
            Type::Int | Type::UnsignedInt | Type::Float => output.push_str(&format!("    .long {}\n", value)),
            Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong
            | Type::Pointer(_) | Type::FunctionPointer { .. } => {
                output.push_str(&format!("    .quad {}\n", value));
            }
            _ => output.push_str(&format!("    .long {}\n", value)),
        }
    }

    /// Emit zero-filled data for a given type.
    pub(crate) fn emit_zero_data(&self, output: &mut String, ty: &Type) {
        let size = self.type_size(ty);
        output.push_str(&format!("    .zero {}\n", size));
    }

    /// Get the size of a type in bytes.
    pub(crate) fn type_size(&self, ty: &Type) -> usize {
        match ty {
            Type::Bool => 1,
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt | Type::Float => 4,
            Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Double => 8,
            Type::Pointer(_) | Type::FunctionPointer { .. } => 8,
            Type::Void => 0,
            Type::Array(inner, size) => self.type_size(inner) * size,
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let s_def = s_def.clone();
                    let is_packed = s_def.attributes.iter().any(|a| matches!(a, model::Attribute::Packed));
                    self.struct_size(&s_def, is_packed)
                } else { 4 }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    u_def.fields.iter().map(|f| self.type_size(&f.field_type)).max().unwrap_or(0)
                } else { 4 }
            }
            Type::Typedef(_) => 4,
            Type::TypeofExpr(_) => 8, // Should be resolved before codegen
        }
    }

    /// Get the alignment of a type in bytes.
    pub(crate) fn type_alignment(&self, ty: &Type) -> usize {
        match ty {
            Type::Bool => 1,
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt | Type::Float => 4,
            Type::Long | Type::UnsignedLong | Type::LongLong | Type::UnsignedLongLong
            | Type::Double | Type::Pointer(_) | Type::FunctionPointer { .. } => 8,
            Type::Array(inner, _) => self.type_alignment(inner),
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    s_def.fields.iter().map(|f| self.type_alignment(&f.field_type)).max().unwrap_or(4)
                } else { 4 }
            }
            _ => 4,
        }
    }

    /// Compute the total size of a struct including padding.
    pub(crate) fn struct_size(&self, s_def: &model::StructDef, is_packed: bool) -> usize {
        let mut size: usize = 0;
        for field in &s_def.fields {
            if !is_packed {
                let align = self.type_alignment(&field.field_type);
                size = (size + align - 1) / align * align;
            }
            size += self.type_size(&field.field_type);
        }
        if !is_packed {
            let align = s_def.fields.iter().map(|f| self.type_alignment(&f.field_type)).max().unwrap_or(4);
            size = (size + align - 1) / align * align;
        }
        size
    }
}

#[cfg(test)]
mod tests {
    use crate::Codegen;
    use model::Type;

    fn cg() -> Codegen { Codegen::new() }

    // ─── type_size ──────────────────────────────────────────────

    #[test]
    fn type_size_primitives() {
        let c = cg();
        assert_eq!(c.type_size(&Type::Bool), 1);
        assert_eq!(c.type_size(&Type::Char), 1);
        assert_eq!(c.type_size(&Type::UnsignedChar), 1);
        assert_eq!(c.type_size(&Type::Short), 2);
        assert_eq!(c.type_size(&Type::UnsignedShort), 2);
        assert_eq!(c.type_size(&Type::Int), 4);
        assert_eq!(c.type_size(&Type::UnsignedInt), 4);
        assert_eq!(c.type_size(&Type::Float), 4);
        assert_eq!(c.type_size(&Type::Long), 8);
        assert_eq!(c.type_size(&Type::UnsignedLong), 8);
        assert_eq!(c.type_size(&Type::Double), 8);
        assert_eq!(c.type_size(&Type::Void), 0);
    }

    #[test]
    fn type_size_pointer() {
        let c = cg();
        assert_eq!(c.type_size(&Type::Pointer(Box::new(Type::Int))), 8);
        assert_eq!(c.type_size(&Type::Pointer(Box::new(Type::Char))), 8);
    }

    #[test]
    fn type_size_array() {
        let c = cg();
        // int[10] = 4 * 10 = 40
        assert_eq!(c.type_size(&Type::Array(Box::new(Type::Int), 10)), 40);
        // char[5] = 1 * 5 = 5
        assert_eq!(c.type_size(&Type::Array(Box::new(Type::Char), 5)), 5);
    }

    #[test]
    fn type_size_nested_array() {
        let c = cg();
        // int[3][4] = 4 * 3 * 4 = 48
        let inner = Type::Array(Box::new(Type::Int), 4);
        assert_eq!(c.type_size(&Type::Array(Box::new(inner), 3)), 48);
    }

    // ─── type_alignment ─────────────────────────────────────────

    #[test]
    fn type_alignment_primitives() {
        let c = cg();
        assert_eq!(c.type_alignment(&Type::Char), 1);
        assert_eq!(c.type_alignment(&Type::Short), 2);
        assert_eq!(c.type_alignment(&Type::Int), 4);
        assert_eq!(c.type_alignment(&Type::Long), 8);
        assert_eq!(c.type_alignment(&Type::Pointer(Box::new(Type::Int))), 8);
    }

    #[test]
    fn type_alignment_array_inherits_element() {
        let c = cg();
        // Array alignment follows element alignment
        assert_eq!(c.type_alignment(&Type::Array(Box::new(Type::Int), 5)), 4);
        assert_eq!(c.type_alignment(&Type::Array(Box::new(Type::Long), 3)), 8);
    }

    // ─── struct_size ────────────────────────────────────────────

    #[test]
    fn struct_size_simple() {
        let c = cg();
        // struct { int a; int b; } → size 8, no padding needed
        let s = model::StructDef {
            name: "test".to_string(),
            fields: vec![
                model::StructField { field_type: Type::Int, name: "a".to_string(), bit_width: None },
                model::StructField { field_type: Type::Int, name: "b".to_string(), bit_width: None },
            ],
            attributes: vec![],
        };
        assert_eq!(c.struct_size(&s, false), 8);
    }

    #[test]
    fn struct_size_with_padding() {
        let c = cg();
        // struct { char a; int b; } → 1 + 3 padding + 4 = 8
        let s = model::StructDef {
            name: "test".to_string(),
            fields: vec![
                model::StructField { field_type: Type::Char, name: "a".to_string(), bit_width: None },
                model::StructField { field_type: Type::Int, name: "b".to_string(), bit_width: None },
            ],
            attributes: vec![],
        };
        assert_eq!(c.struct_size(&s, false), 8);
    }

    #[test]
    fn struct_size_with_trailing_padding() {
        let c = cg();
        // struct { long a; char b; } → 8 + 1 + 7 trailing = 16
        let s = model::StructDef {
            name: "test".to_string(),
            fields: vec![
                model::StructField { field_type: Type::Long, name: "a".to_string(), bit_width: None },
                model::StructField { field_type: Type::Char, name: "b".to_string(), bit_width: None },
            ],
            attributes: vec![],
        };
        assert_eq!(c.struct_size(&s, false), 16);
    }

    #[test]
    fn struct_size_packed() {
        let c = cg();
        // __attribute__((packed)) struct { char a; int b; } → 1 + 4 = 5 (no padding)
        let s = model::StructDef {
            name: "test".to_string(),
            fields: vec![
                model::StructField { field_type: Type::Char, name: "a".to_string(), bit_width: None },
                model::StructField { field_type: Type::Int, name: "b".to_string(), bit_width: None },
            ],
            attributes: vec![model::Attribute::Packed],
        };
        assert_eq!(c.struct_size(&s, true), 5);
    }

    // ─── emit_scalar_data ───────────────────────────────────────

    #[test]
    fn emit_scalar_byte() {
        let c = cg();
        let mut out = String::new();
        c.emit_scalar_data(&mut out, &Type::Char, 65);
        assert_eq!(out, "    .byte 65\n");
    }

    #[test]
    fn emit_scalar_short() {
        let c = cg();
        let mut out = String::new();
        c.emit_scalar_data(&mut out, &Type::Short, 1234);
        assert_eq!(out, "    .short 1234\n");
    }

    #[test]
    fn emit_scalar_int() {
        let c = cg();
        let mut out = String::new();
        c.emit_scalar_data(&mut out, &Type::Int, 42);
        assert_eq!(out, "    .long 42\n");
    }

    #[test]
    fn emit_scalar_long() {
        let c = cg();
        let mut out = String::new();
        c.emit_scalar_data(&mut out, &Type::Long, 1_000_000);
        assert_eq!(out, "    .quad 1000000\n");
    }

    #[test]
    fn emit_scalar_pointer() {
        let c = cg();
        let mut out = String::new();
        c.emit_scalar_data(&mut out, &Type::Pointer(Box::new(Type::Void)), 0);
        assert_eq!(out, "    .quad 0\n");
    }

    // ─── emit_zero_data ─────────────────────────────────────────

    #[test]
    fn emit_zero_int() {
        let c = cg();
        let mut out = String::new();
        c.emit_zero_data(&mut out, &Type::Int);
        assert_eq!(out, "    .zero 4\n");
    }

    #[test]
    fn emit_zero_array() {
        let c = cg();
        let mut out = String::new();
        c.emit_zero_data(&mut out, &Type::Array(Box::new(Type::Int), 10));
        assert_eq!(out, "    .zero 40\n");
    }
}
