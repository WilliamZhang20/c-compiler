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
            Type::Bool => 1,
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
            Type::TypeofExpr(_) => 8, // Should be resolved before codegen
        }
    }

    pub fn get_type_size(&self, r#type: &Type) -> usize {
        match r#type {
            Type::Int | Type::UnsignedInt => 4,  // 32-bit int
            Type::Bool => 1,  // _Bool
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
            Type::TypeofExpr(_) => 8, // Should be resolved before codegen
        }
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_calc() -> TypeCalculator<'static> {
        // Use leaked boxes to get 'static lifetime for test convenience
        let structs: &'static HashMap<String, model::StructDef> = Box::leak(Box::new(HashMap::new()));
        let unions: &'static HashMap<String, model::UnionDef> = Box::leak(Box::new(HashMap::new()));
        TypeCalculator::new(structs, unions)
    }

    // ─── Type sizes ─────────────────────────────────────────────
    #[test]
    fn size_int() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Int), 4);
        assert_eq!(calc.get_type_size(&Type::UnsignedInt), 4);
    }

    #[test]
    fn size_char() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Char), 1);
        assert_eq!(calc.get_type_size(&Type::UnsignedChar), 1);
    }

    #[test]
    fn size_short() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Short), 2);
        assert_eq!(calc.get_type_size(&Type::UnsignedShort), 2);
    }

    #[test]
    fn size_long() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Long), 8);
        assert_eq!(calc.get_type_size(&Type::UnsignedLong), 8);
    }

    #[test]
    fn size_long_long() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::LongLong), 8);
        assert_eq!(calc.get_type_size(&Type::UnsignedLongLong), 8);
    }

    #[test]
    fn size_bool() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Bool), 1);
    }

    #[test]
    fn size_float_double() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Float), 4);
        assert_eq!(calc.get_type_size(&Type::Double), 8);
    }

    #[test]
    fn size_void() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Void), 0);
    }

    #[test]
    fn size_pointer() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Pointer(Box::new(Type::Int))), 8);
        assert_eq!(calc.get_type_size(&Type::Pointer(Box::new(Type::Char))), 8);
    }

    #[test]
    fn size_array() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Array(Box::new(Type::Int), 10)), 40);
        assert_eq!(calc.get_type_size(&Type::Array(Box::new(Type::Char), 5)), 5);
    }

    #[test]
    fn size_function_pointer() {
        let calc = empty_calc();
        let fp = Type::FunctionPointer {
            return_type: Box::new(Type::Int),
            param_types: vec![Type::Int],
        };
        assert_eq!(calc.get_type_size(&fp), 8);
    }

    #[test]
    fn size_typedef_known() {
        let calc = empty_calc();
        assert_eq!(calc.get_type_size(&Type::Typedef("size_t".to_string())), 8);
        assert_eq!(calc.get_type_size(&Type::Typedef("int32_t".to_string())), 4);
        assert_eq!(calc.get_type_size(&Type::Typedef("uint8_t".to_string())), 1);
        assert_eq!(calc.get_type_size(&Type::Typedef("int16_t".to_string())), 2);
    }

    // ─── Alignment ──────────────────────────────────────────────
    #[test]
    fn align_primitives() {
        let calc = empty_calc();
        assert_eq!(calc.get_alignment(&Type::Bool), 1);
        assert_eq!(calc.get_alignment(&Type::Char), 1);
        assert_eq!(calc.get_alignment(&Type::Short), 2);
        assert_eq!(calc.get_alignment(&Type::Int), 4);
        assert_eq!(calc.get_alignment(&Type::Long), 8);
        assert_eq!(calc.get_alignment(&Type::Double), 8);
        assert_eq!(calc.get_alignment(&Type::Pointer(Box::new(Type::Int))), 8);
    }

    #[test]
    fn align_array() {
        let calc = empty_calc();
        // Array alignment is the element alignment
        assert_eq!(calc.get_alignment(&Type::Array(Box::new(Type::Int), 10)), 4);
        assert_eq!(calc.get_alignment(&Type::Array(Box::new(Type::Char), 5)), 1);
    }

    // ─── Struct layout ──────────────────────────────────────────
    #[test]
    fn size_struct_simple() {
        let mut structs = HashMap::new();
        structs.insert("Point".to_string(), model::StructDef {
            name: "Point".to_string(),
            fields: vec![
                model::StructField { name: "x".to_string(), field_type: Type::Int, bit_width: None },
                model::StructField { name: "y".to_string(), field_type: Type::Int, bit_width: None },
            ],
            attributes: vec![],
        });
        let unions = HashMap::new();
        let calc = TypeCalculator::new(&structs, &unions);
        assert_eq!(calc.get_type_size(&Type::Struct("Point".to_string())), 8);
    }

    #[test]
    fn size_struct_with_padding() {
        let mut structs = HashMap::new();
        structs.insert("Mixed".to_string(), model::StructDef {
            name: "Mixed".to_string(),
            fields: vec![
                model::StructField { name: "c".to_string(), field_type: Type::Char, bit_width: None },
                model::StructField { name: "i".to_string(), field_type: Type::Int, bit_width: None },
            ],
            attributes: vec![],
        });
        let unions = HashMap::new();
        let calc = TypeCalculator::new(&structs, &unions);
        // char(1) + 3 padding + int(4) = 8
        assert_eq!(calc.get_type_size(&Type::Struct("Mixed".to_string())), 8);
    }

    #[test]
    fn size_struct_packed() {
        let mut structs = HashMap::new();
        structs.insert("Packed".to_string(), model::StructDef {
            name: "Packed".to_string(),
            fields: vec![
                model::StructField { name: "c".to_string(), field_type: Type::Char, bit_width: None },
                model::StructField { name: "i".to_string(), field_type: Type::Int, bit_width: None },
            ],
            attributes: vec![model::Attribute::Packed],
        });
        let unions = HashMap::new();
        let calc = TypeCalculator::new(&structs, &unions);
        // packed: char(1) + int(4) = 5 (no padding)
        assert_eq!(calc.get_type_size(&Type::Struct("Packed".to_string())), 5);
    }

    #[test]
    fn align_struct_packed() {
        let mut structs = HashMap::new();
        structs.insert("Packed".to_string(), model::StructDef {
            name: "Packed".to_string(),
            fields: vec![
                model::StructField { name: "c".to_string(), field_type: Type::Char, bit_width: None },
                model::StructField { name: "i".to_string(), field_type: Type::Int, bit_width: None },
            ],
            attributes: vec![model::Attribute::Packed],
        });
        let unions = HashMap::new();
        let calc = TypeCalculator::new(&structs, &unions);
        assert_eq!(calc.get_alignment(&Type::Struct("Packed".to_string())), 1);
    }

    // ─── Union layout ───────────────────────────────────────────
    #[test]
    fn size_union() {
        let structs = HashMap::new();
        let mut unions = HashMap::new();
        unions.insert("Data".to_string(), model::UnionDef {
            name: "Data".to_string(),
            fields: vec![
                model::StructField { name: "i".to_string(), field_type: Type::Int, bit_width: None },
                model::StructField { name: "d".to_string(), field_type: Type::Double, bit_width: None },
            ],
        });
        let calc = TypeCalculator::new(&structs, &unions);
        // Union takes max size of fields
        assert_eq!(calc.get_type_size(&Type::Union("Data".to_string())), 8);
    }
}
