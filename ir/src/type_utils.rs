// Type utility methods for the IR lowerer
// Extracted from lowerer.rs: get_type_size, get_alignment, is_float_type, get_member_offset

use model::Type;
use crate::lowerer::Lowerer;

impl Lowerer {
    /// Calculate the size of a type in bytes
    pub(crate) fn get_type_size(&mut self, ty: &Type) -> i64 {
        // Create a cache key from the type
        let cache_key = format!("{:?}", ty);
        
        // Check cache first
        if let Some(&size) = self.type_size_cache.get(&cache_key) {
            return size;
        }
        
        // Compute size
        let size = match ty {
            Type::Int | Type::UnsignedInt => 4,  // 32-bit int
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Long | Type::UnsignedLong => 8,  // 64-bit on x64
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,  // 32-bit float
            Type::Double => 8, // 64-bit double
            Type::Void => 0,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8, // Function pointers are 8 bytes
            Type::Array(base, size) => self.get_type_size(base) * (*size as i64),
            Type::Struct(name) => {
                if let Some(s_def) = self.struct_defs.get(name).cloned() {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    let mut size = 0;
                    
                    for field in &s_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        
                        // Align field if not packed
                        if !is_packed {
                            let alignment = self.get_alignment(&field.field_type);
                            // Align current size to field alignment
                            size = ((size + alignment - 1) / alignment) * alignment;
                        }
                        
                        size += field_size;
                    }
                    
                    // Add padding to make struct size a multiple of its alignment
                    if !is_packed {
                        let struct_alignment = self.get_alignment(ty);
                        size = ((size + struct_alignment - 1) / struct_alignment) * struct_alignment;
                    }
                    
                    size
                } else {
                    4 // fallback or error
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.union_defs.get(name).cloned() {
                    // Union size is the largest field
                    let mut max_size = 0;
                    for field in &u_def.fields {
                        let field_size = self.get_type_size(&field.field_type);
                        if field_size > max_size {
                            max_size = field_size;
                        }
                    }
                    max_size
                } else {
                    4 // fallback
                }
            }
            Type::Typedef(name) => {
                if let Some(real_ty) = self.typedefs.get(name).cloned() {
                    self.get_type_size(&real_ty)
                } else {
                    4
                }
            }
        };
        
        // Cache the result
        self.type_size_cache.insert(cache_key, size);
        size
    }

    /// Get the natural alignment of a type in bytes
    pub(crate) fn get_alignment(&self, ty: &Type) -> i64 {
        match ty {
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
                if let Some(s_def) = self.struct_defs.get(name) {
                    let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
                    if is_packed {
                        return 1; // Packed structs have alignment 1
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
                if let Some(u_def) = self.union_defs.get(name) {
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
            Type::Typedef(name) => {
                if let Some(real_ty) = self.typedefs.get(name) {
                    self.get_alignment(real_ty)
                } else {
                    4
                }
            }
            Type::Void => 1,
        }
    }

    /// Check if a type is a floating-point type
    pub(crate) fn is_float_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Float | Type::Double)
    }

    /// Get the byte offset and type of a struct/union member
    pub(crate) fn get_member_offset(&mut self, struct_or_union_name: &str, member_name: &str) -> (i64, Type) {
        // Check if it's a struct
        if let Some(s_def) = self.struct_defs.get(struct_or_union_name).cloned() {
            let is_packed = s_def.attributes.iter().any(|attr| matches!(attr, model::Attribute::Packed));
            let mut offset = 0;
            
            for field in &s_def.fields {
                // Align the offset if not packed
                if !is_packed {
                    let alignment = self.get_alignment(&field.field_type);
                    // Round up to next aligned boundary
                    offset = ((offset + alignment - 1) / alignment) * alignment;
                }
                
                if &field.name == member_name {
                    return (offset, field.field_type.clone());
                }
                offset += self.get_type_size(&field.field_type);
            }
        }
        // Check if it's a union (all fields at offset 0)
        if let Some (u_def) = self.union_defs.get(struct_or_union_name) {
            for field in &u_def.fields {
                if &field.name == member_name {
                    return (0, field.field_type.clone());  // All union fields start at offset 0
                }
            }
        }
        (0, Type::Int)
    }
}
