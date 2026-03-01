// Centralized type layout computation
//
// This module provides canonical type size, alignment, and member offset
// calculations used by IR lowering, optimization, and code generation.
// Having a single implementation eliminates the previous triple duplication
// across ir/type_utils.rs, codegen/types.rs, and codegen/globals.rs.

use crate::{Type, StructDef, UnionDef, Attribute, BitfieldInfo};
use std::collections::HashMap;

/// Provides type size and alignment computation for a given set of struct/union definitions.
pub struct TypeLayout<'a> {
    pub structs: &'a HashMap<String, StructDef>,
    pub unions: &'a HashMap<String, UnionDef>,
    /// Optional typedef resolution map (typedef name -> resolved type)
    pub typedefs: Option<&'a HashMap<String, Type>>,
}

impl<'a> TypeLayout<'a> {
    pub fn new(
        structs: &'a HashMap<String, StructDef>,
        unions: &'a HashMap<String, UnionDef>,
    ) -> Self {
        Self { structs, unions, typedefs: None }
    }

    pub fn with_typedefs(
        structs: &'a HashMap<String, StructDef>,
        unions: &'a HashMap<String, UnionDef>,
        typedefs: &'a HashMap<String, Type>,
    ) -> Self {
        Self { structs, unions, typedefs: Some(typedefs) }
    }

    /// Calculate the size of a type in bytes.
    pub fn size_of(&self, ty: &Type) -> usize {
        match ty {
            Type::Bool => 1,
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt | Type::Enum(_) => 4,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,
            Type::Double => 8,
            Type::Void => 0,
            Type::Pointer(_, ..) | Type::FunctionPointer { .. } => 8,
            Type::Array(inner, count) => self.size_of(inner) * count,
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let is_packed = s_def.attributes.iter()
                        .any(|attr| matches!(attr, Attribute::Packed));
                    self.struct_size(s_def, is_packed)
                } else {
                    4 // fallback for unknown struct
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    u_def.fields.iter()
                        .map(|f| self.size_of(&f.field_type))
                        .max()
                        .unwrap_or(0)
                } else {
                    4 // fallback for unknown union
                }
            }
            Type::Typedef(name) => {
                // Try typedef resolution first
                if let Some(typedefs) = self.typedefs {
                    if let Some(real_ty) = typedefs.get(name) {
                        return self.size_of(real_ty);
                    }
                }
                // Fallback: well-known typedefs
                match name.as_str() {
                    "int8_t" | "uint8_t" | "int8" | "uint8" => 1,
                    "int16_t" | "uint16_t" | "int16" | "uint16" => 2,
                    "int32_t" | "uint32_t" | "int32" | "uint32" => 4,
                    "int64_t" | "uint64_t" | "int64" | "uint64"
                    | "size_t" | "ssize_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" => 8,
                    _ => 4,
                }
            }
            Type::TypeofExpr(_) => 8, // Should be resolved before layout computation
        }
    }

    /// Get the natural alignment of a type in bytes.
    pub fn align_of(&self, ty: &Type) -> usize {
        match ty {
            Type::Bool => 1,
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Int | Type::UnsignedInt | Type::Enum(_) => 4,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Float => 4,
            Type::Double => 8,
            Type::Pointer(_, ..) | Type::FunctionPointer { .. } => 8,
            Type::Array(inner, _) => self.align_of(inner),
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let is_packed = s_def.attributes.iter()
                        .any(|attr| matches!(attr, Attribute::Packed));
                    if is_packed {
                        return 1;
                    }
                    s_def.fields.iter()
                        .map(|f| self.align_of(&f.field_type))
                        .max()
                        .unwrap_or(4)
                } else {
                    4
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    u_def.fields.iter()
                        .map(|f| self.align_of(&f.field_type))
                        .max()
                        .unwrap_or(4)
                } else {
                    4
                }
            }
            Type::Typedef(name) => {
                if let Some(typedefs) = self.typedefs {
                    if let Some(real_ty) = typedefs.get(name) {
                        return self.align_of(real_ty);
                    }
                }
                match name.as_str() {
                    "int8_t" | "uint8_t" | "int8" | "uint8" => 1,
                    "int16_t" | "uint16_t" | "int16" | "uint16" => 2,
                    "int32_t" | "uint32_t" | "int32" | "uint32" => 4,
                    "int64_t" | "uint64_t" | "int64" | "uint64"
                    | "size_t" | "ssize_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" => 8,
                    _ => 4,
                }
            }
            Type::Void => 1,
            Type::TypeofExpr(_) => 8,
        }
    }

    /// Compute the total size of a struct including field alignment padding and bitfield packing.
    pub fn struct_size(&self, s_def: &StructDef, is_packed: bool) -> usize {
        let mut size: usize = 0;
        let mut bit_offset: usize = 0; // bits used within current storage unit
        let mut in_bitfield = false;
        let mut bf_storage_size: usize = 0;

        for field in &s_def.fields {
            if let Some(bw) = field.bit_width {
                let storage = self.size_of(&field.field_type);
                let storage_bits = storage * 8;
                if bw == 0 {
                    // Zero-width bitfield = force alignment to next storage unit boundary
                    if in_bitfield {
                        size += bf_storage_size;
                        bit_offset = 0;
                        in_bitfield = false;
                    }
                    continue;
                }
                if in_bitfield && bf_storage_size == storage && bit_offset + bw <= storage_bits {
                    // Fits in current storage unit
                    bit_offset += bw;
                } else {
                    // Finish previous storage unit if any
                    if in_bitfield {
                        size += bf_storage_size;
                    } else if !is_packed {
                        let alignment = self.align_of(&field.field_type);
                        size = (size + alignment - 1) / alignment * alignment;
                    }
                    bf_storage_size = storage;
                    bit_offset = bw;
                    in_bitfield = true;
                }
            } else {
                // Regular field — finish any pending bitfield storage
                if in_bitfield {
                    size += bf_storage_size;
                    bit_offset = 0;
                    in_bitfield = false;
                }
                let field_size = self.size_of(&field.field_type);
                if !is_packed {
                    let alignment = self.align_of(&field.field_type);
                    size = (size + alignment - 1) / alignment * alignment;
                }
                size += field_size;
            }
        }
        // Finish any trailing bitfield
        if in_bitfield {
            size += bf_storage_size;
        }
        // Add trailing padding
        if !is_packed {
            let struct_align = s_def.fields.iter()
                .map(|f| self.align_of(&f.field_type))
                .max()
                .unwrap_or(1);
            size = (size + struct_align - 1) / struct_align * struct_align;
        }
        size
    }

    /// Get the byte offset and type of a struct/union member, plus optional bitfield info.
    pub fn member_offset(&self, struct_or_union_name: &str, member_name: &str) -> (usize, Type, Option<BitfieldInfo>) {
        // Check structs
        if let Some(s_def) = self.structs.get(struct_or_union_name) {
            let is_packed = s_def.attributes.iter()
                .any(|attr| matches!(attr, Attribute::Packed));
            let mut offset: usize = 0;
            let mut bit_offset: usize = 0;
            let mut in_bitfield = false;
            let mut bf_storage_size: usize = 0;

            for field in &s_def.fields {
                if let Some(bw) = field.bit_width {
                    let storage = self.size_of(&field.field_type);
                    let storage_bits = storage * 8;
                    if bw == 0 {
                        if in_bitfield {
                            offset += bf_storage_size;
                            bit_offset = 0;
                            in_bitfield = false;
                        }
                        continue;
                    }
                    if in_bitfield && bf_storage_size == storage && bit_offset + bw <= storage_bits {
                        // Fits in current storage unit
                        if field.name == member_name {
                            return (offset, field.field_type.clone(), Some(BitfieldInfo {
                                bit_offset,
                                bit_width: bw,
                                storage_size: storage,
                            }));
                        }
                        bit_offset += bw;
                    } else {
                        // New storage unit
                        if in_bitfield {
                            offset += bf_storage_size;
                        } else if !is_packed {
                            let alignment = self.align_of(&field.field_type);
                            offset = (offset + alignment - 1) / alignment * alignment;
                        }
                        bf_storage_size = storage;
                        if field.name == member_name {
                            return (offset, field.field_type.clone(), Some(BitfieldInfo {
                                bit_offset: 0,
                                bit_width: bw,
                                storage_size: storage,
                            }));
                        }
                        bit_offset = bw;
                        in_bitfield = true;
                    }
                } else {
                    // Regular field
                    if in_bitfield {
                        offset += bf_storage_size;
                        bit_offset = 0;
                        in_bitfield = false;
                    }
                    if !is_packed {
                        let alignment = self.align_of(&field.field_type);
                        offset = (offset + alignment - 1) / alignment * alignment;
                    }
                    if field.name == member_name {
                        return (offset, field.field_type.clone(), None);
                    }
                    offset += self.size_of(&field.field_type);
                }
            }
        }
        // Check unions (all fields at offset 0)
        if let Some(u_def) = self.unions.get(struct_or_union_name) {
            for field in &u_def.fields {
                if field.name == member_name {
                    return (0, field.field_type.clone(), None);
                }
            }
        }
        (0, Type::Int, None) // fallback
    }

    /// Check if a type is a floating-point type.
    pub fn is_float_type(ty: &Type) -> bool {
        matches!(ty, Type::Float | Type::Double)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StructField;

    fn empty_layout() -> TypeLayout<'static> {
        let structs: &'static HashMap<String, StructDef> = Box::leak(Box::new(HashMap::new()));
        let unions: &'static HashMap<String, UnionDef> = Box::leak(Box::new(HashMap::new()));
        TypeLayout::new(structs, unions)
    }

    #[test]
    fn test_primitive_sizes() {
        let layout = empty_layout();
        assert_eq!(layout.size_of(&Type::Int), 4);
        assert_eq!(layout.size_of(&Type::Char), 1);
        assert_eq!(layout.size_of(&Type::Long), 8);
        assert_eq!(layout.size_of(&Type::Float), 4);
        assert_eq!(layout.size_of(&Type::Double), 8);
        assert_eq!(layout.size_of(&Type::Bool), 1);
        assert_eq!(layout.size_of(&Type::Short), 2);
        assert_eq!(layout.size_of(&Type::ptr(Type::Int)), 8);
    }

    #[test]
    fn test_array_size() {
        let layout = empty_layout();
        assert_eq!(layout.size_of(&Type::Array(Box::new(Type::Int), 10)), 40);
        assert_eq!(layout.size_of(&Type::Array(Box::new(Type::Char), 5)), 5);
    }

    #[test]
    fn test_struct_layout() {
        let mut structs = HashMap::new();
        structs.insert("Point".to_string(), StructDef {
            name: "Point".to_string(),
            fields: vec![
                StructField { field_type: Type::Int, name: "x".to_string(), bit_width: None },
                StructField { field_type: Type::Int, name: "y".to_string(), bit_width: None },
            ],
            attributes: vec![],
        });
        let unions = HashMap::new();
        let layout = TypeLayout::new(&structs, &unions);
        assert_eq!(layout.size_of(&Type::Struct("Point".to_string())), 8);
        assert_eq!(layout.align_of(&Type::Struct("Point".to_string())), 4);

        let (offset_x, _, _) = layout.member_offset("Point", "x");
        let (offset_y, _, _) = layout.member_offset("Point", "y");
        assert_eq!(offset_x, 0);
        assert_eq!(offset_y, 4);
    }

    #[test]
    fn test_struct_padding() {
        let mut structs = HashMap::new();
        structs.insert("Padded".to_string(), StructDef {
            name: "Padded".to_string(),
            fields: vec![
                StructField { field_type: Type::Char, name: "c".to_string(), bit_width: None },
                StructField { field_type: Type::Int, name: "i".to_string(), bit_width: None },
            ],
            attributes: vec![],
        });
        let unions = HashMap::new();
        let layout = TypeLayout::new(&structs, &unions);
        // char(1) + 3 padding + int(4) = 8
        assert_eq!(layout.size_of(&Type::Struct("Padded".to_string())), 8);
        let (offset_i, _, _) = layout.member_offset("Padded", "i");
        assert_eq!(offset_i, 4); // aligned to 4
    }

    #[test]
    fn test_packed_struct() {
        let mut structs = HashMap::new();
        structs.insert("Packed".to_string(), StructDef {
            name: "Packed".to_string(),
            fields: vec![
                StructField { field_type: Type::Char, name: "c".to_string(), bit_width: None },
                StructField { field_type: Type::Int, name: "i".to_string(), bit_width: None },
            ],
            attributes: vec![Attribute::Packed],
        });
        let unions = HashMap::new();
        let layout = TypeLayout::new(&structs, &unions);
        // packed: char(1) + int(4) = 5, no padding
        assert_eq!(layout.size_of(&Type::Struct("Packed".to_string())), 5);
        assert_eq!(layout.align_of(&Type::Struct("Packed".to_string())), 1);
    }

    #[test]
    fn test_union_size() {
        let structs = HashMap::new();
        let mut unions = HashMap::new();
        unions.insert("Data".to_string(), UnionDef {
            name: "Data".to_string(),
            fields: vec![
                StructField { field_type: Type::Int, name: "i".to_string(), bit_width: None },
                StructField { field_type: Type::Double, name: "d".to_string(), bit_width: None },
            ],
        });
        let layout = TypeLayout::new(&structs, &unions);
        assert_eq!(layout.size_of(&Type::Union("Data".to_string())), 8);
    }

    #[test]
    fn test_alignments() {
        let layout = empty_layout();
        assert_eq!(layout.align_of(&Type::Char), 1);
        assert_eq!(layout.align_of(&Type::Short), 2);
        assert_eq!(layout.align_of(&Type::Int), 4);
        assert_eq!(layout.align_of(&Type::Long), 8);
        assert_eq!(layout.align_of(&Type::Double), 8);
    }
}
