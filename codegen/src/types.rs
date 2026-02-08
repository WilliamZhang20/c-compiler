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
                    let mut size = 0;
                    for (f_ty, _) in &s_def.fields {
                        size += self.get_type_size(f_ty);
                    }
                    size
                } else {
                    8
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    let mut max_size = 0;
                    for (f_ty, _) in &u_def.fields {
                        let field_size = self.get_type_size(f_ty);
                        if field_size > max_size {
                            max_size = field_size;
                        }
                    }
                    max_size
                } else {
                    8
                }
            }
            Type::Typedef(_) => 8,
        }
    }

    pub fn get_element_size(&self, r#type: &Type) -> usize {
        match r#type {
            Type::Int | Type::UnsignedInt => 4,
            Type::Char | Type::UnsignedChar => 1,
            Type::Short | Type::UnsignedShort => 2,
            Type::Long | Type::UnsignedLong => 8,
            Type::LongLong | Type::UnsignedLongLong => 8,
            Type::Void => 0,
            Type::Float => 4,
            Type::Double => 8,
            Type::Array(inner, size) => self.get_element_size(inner) * size,
            Type::Pointer(_) => 8,
            Type::FunctionPointer { .. } => 8,
            Type::Struct(name) => {
                if let Some(s_def) = self.structs.get(name) {
                    let mut size = 0;
                    for (f_ty, _) in &s_def.fields {
                        size += self.get_element_size(f_ty);
                    }
                    size
                } else {
                    8
                }
            }
            Type::Union(name) => {
                if let Some(u_def) = self.unions.get(name) {
                    let mut max_size = 0;
                    for (f_ty, _) in &u_def.fields {
                        let field_size = self.get_element_size(f_ty);
                        if field_size > max_size {
                            max_size = field_size;
                        }
                    }
                    max_size
                } else {
                    8
                }
            }
            Type::Typedef(_) => 8,
        }
    }
}
