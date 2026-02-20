use model::Type;
use std::collections::HashMap;

/// Type size calculation utilities for code generation
pub struct TypeCalculator<'a> {
    pub structs: &'a HashMap<String, model::StructDef>,
    pub unions: &'a HashMap<String, model::UnionDef>,
}

impl<'a> TypeCalculator<'a> {
    pub fn new(
        structs: &'a HashMap<String, model::StructDef>,
        unions: &'a HashMap<String, model::UnionDef>,
    ) -> Self {
        Self { structs, unions }
    }

    pub fn get_alignment(&self, r#type: &Type) -> usize {
        match r#type {
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt => 4,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,
            Type::Double => 8,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8,
            Type::Array(base, _) => self.get_alignment(base),
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    if is_packed {
                        return 1;
                    }
                    let mut max_alignment = 1;
                    for field in &s_def.fields {
                        let field_align = self.get_alignment(&field.field_type);
                        if field_align > max_alignment {
                            max_alignment = field_align;
                        }
                    }
                    max_alignment
                } else {
                    4
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    let mut max_alignment = 1;
                    for field in &u_def.fields {
                        let field_align = self.get_alignment(&field.field_type);
                        if field_align > max_alignment {
                            max_alignment = field_align;
                        }
                    }
                    max_alignment
                } else {
                    4
                }
            }
            Type::Typedef(name) => match name.as_str() {
                "int8_t" | "uint8_t" | "int8" | "uint8" => 1,
                "int16_t" | "uint16_t" | "int16" | "uint16" => 2,
                "int32_t" | "uint32_t" | "int32" | "uint32" => 4,
                "int64_t" | "uint64_t" | "int64" | "uint64"
                | "size_t" | "ssize_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" => 8,
                _ => 4,
            },
            Type::Void => 1,
        }
    }

    pub fn get_type_size(&self, r#type: &Type) -> usize {
        match r#type {
            Type::Int | Type::UnsignedInt => 4,  // 32-bit int
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Void => 0,
            Type::Float => 4,  // 32-bit float
            Type::Double => 8, // 64-bit double
            Type::Array(inner, size) => self.get_type_size(inner) * size,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8,
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    let mut size = 0;
                    
                    for field in &s_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        
                        // Align field if not packed
                        if !is_packed {
                            let alignment = self.get_alignment(&field.field_type);
                            size = ((size + alignment - 1) / alignment) * alignment;
                        }
                        
                        size += field_size;
                    }
                    
                    // Add padding to make struct size a multiple of its alignment
                    if !is_packed {
                        let struct_alignment = self.get_alignment(r#type);
                        size = ((size + struct_alignment - 1) / struct_alignment) * struct_alignment;
                    }
                    
                    size
                } else {
                    8
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    let mut max_size = 0;
                    for field in &u_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        if field_size > max_size {
                            max_size = field_size;
                        }
                    }
                    max_size
                } else {
                    8
                }
            }
            Type::Typedef(name) => match name.as_str() {
                "int8_t" | "uint8_t" | "int8" | "uint8" => 1,
                "int16_t" | "uint16_t" | "int16" | "uint16" => 2,
                "int32_t" | "uint32_t" | "int32" | "uint32" => 4,
                "int64_t" | "uint64_t" | "int64" | "uint64"
                | "size_t" | "ssize_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" => 8,
                _ => 8,
            },
        }
    }


}
