// Type utility methods for the IR lowerer
// Delegates core layout computation to model::TypeLayout, keeping a name-based
// cache for complex types (struct/union/typedef) whose size is queried repeatedly.

use model::{Type, TypeLayout};
use crate::lowerer::Lowerer;

impl Lowerer {
    /// Build a TypeLayout that borrows from the lowerer's definition maps.
    fn type_layout(&self) -> TypeLayout<'_> {
        TypeLayout::with_typedefs(&self.struct_defs, &self.union_defs, &self.typedefs)
    }

    /// Calculate the size of a type in bytes.
    pub(crate) fn get_type_size(&mut self, ty: &Type) -> i64 {
        // Fast path – leaf types never need the cache
        match ty {
            Type::Int | Type::UnsignedInt | Type::Enum(_) => return 4,
            Type::Bool => return 1,
            Type::Char | Type::UnsignedChar => return 1,
            Type::Short | Type::UnsignedShort => return 2,
            Type::Long | Type::UnsignedLong => return 8,
            Type::LongLong | Type::UnsignedLongLong => return 8,
            Type::Float => return 4,
            Type::Double => return 8,
            Type::Void => return 0,
            Type::Pointer(_, ..) | Type::FunctionPointer { .. } => return 8,
            _ => {}
        }

        // Name-based cache for struct/union/typedef
        let cache_key = match ty {
            Type::Struct(name) | Type::Union(name) | Type::Typedef(name) => Some(name.clone()),
            _ => None,
        };

        if let Some(ref key) = cache_key {
            if let Some(&size) = self.type_size_cache.get(key) {
                return size;
            }
        }

        // TypeofExpr still needs lowerer-specific resolution
        let size = match ty {
            Type::TypeofExpr(expr) => {
                let resolved = self.get_expr_type(expr);
                self.get_type_size(&resolved)
            }
            Type::Array(base, count) => {
                // Array needs recursive call for potential caching
                self.get_type_size(base) * (*count as i64)
            }
            other => self.type_layout().size_of(other) as i64,
        };

        if let Some(key) = cache_key {
            self.type_size_cache.insert(key, size);
        }
        size
    }

    /// Get the natural alignment of a type in bytes.
    pub(crate) fn get_alignment(&self, ty: &Type) -> i64 {
        match ty {
            Type::TypeofExpr(expr) => {
                let resolved = self.get_expr_type(expr);
                self.get_alignment(&resolved)
            }
            _ => self.type_layout().align_of(ty) as i64,
        }
    }

    /// Check if a type is a floating-point type.
    pub(crate) fn is_float_type(&self, ty: &Type) -> bool {
        TypeLayout::is_float_type(ty)
    }

    /// Get the byte offset and type of a struct/union member, plus optional bitfield info.
    pub(crate) fn get_member_offset(&mut self, struct_or_union_name: &str, member_name: &str) -> (i64, Type, Option<model::BitfieldInfo>) {
        let (offset, ty, bf_info) = self.type_layout().member_offset(struct_or_union_name, member_name);
        (offset as i64, ty, bf_info)
    }

    /// Check if two types are compatible for _Generic matching.
    pub(crate) fn types_compatible(&self, a: &Type, b: &Type) -> bool {
        match (a, b) {
            (Type::Int, Type::Int) => true,
            (Type::Enum(_), Type::Int) | (Type::Int, Type::Enum(_)) => true,
            (Type::Enum(a), Type::Enum(b)) if a == b => true,
            (Type::Char, Type::Char) => true,
            (Type::Short, Type::Short) => true,
            (Type::Long, Type::Long) => true,
            (Type::Float, Type::Float) => true,
            (Type::Double, Type::Double) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Void, Type::Void) => true,
            (Type::UnsignedChar, Type::UnsignedChar) => true,
            (Type::UnsignedShort, Type::UnsignedShort) => true,
            (Type::UnsignedInt, Type::UnsignedInt) => true,
            (Type::UnsignedLong, Type::UnsignedLong) => true,
            (Type::Pointer(a_inner, ..), Type::Pointer(b_inner, ..)) => self.types_compatible(a_inner, b_inner),
            (Type::Array(a_inner, _), Type::Array(b_inner, _)) => self.types_compatible(a_inner, b_inner),
            (Type::Struct(a_name), Type::Struct(b_name)) => a_name == b_name,
            (Type::Union(a_name), Type::Union(b_name)) => a_name == b_name,
            _ => false,
        }
    }
}
