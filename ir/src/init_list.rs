use model::{BinaryOp, Type, Expr as AstExpr};
use crate::types::{VarId, BlockId, Operand, Instruction};
use crate::lowerer::Lowerer;

/// Initializer list lowering implementation
impl Lowerer {
    /// Lower an array initializer list to a sequence of GEP+Store instructions.
    /// `base_var` is the alloca'd array address.
    pub(crate) fn lower_init_list_to_stores(
        &mut self,
        base_var: VarId,
        items: &[model::InitItem],
        elem_type: &Type,
        elem_size: i64,
        bid: BlockId,
    ) -> Result<(), String> {
        for item in items {
            let index = match &item.designator {
                Some(model::Designator::Index(idx)) => *idx as usize,
                Some(model::Designator::Field(_)) => {
                    return Err("Field designator not valid in array initializer".to_string());
                }
                None => {
                    // Positional: determine position from how many items we've stored so far
                    // We rely on the caller to pass items in order; compute from slice position
                    let pos = items.iter().position(|x| std::ptr::eq(x, item)).unwrap_or(0);
                    pos
                }
            };

            let byte_offset = (index as i64) * elem_size;
            let dest_var = if byte_offset == 0 {
                base_var
            } else {
                let offset_var = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::Binary {
                    dest: offset_var,
                    op: BinaryOp::Add,
                    left: Operand::Var(base_var),
                    right: Operand::Constant(byte_offset),
                });
                offset_var
            };

            // Handle nested init lists (e.g., 2D arrays or array of structs)
            match &item.value {
                AstExpr::InitList(nested_items) => {
                    // For nested array: inner element type and size
                    match elem_type {
                        Type::Array(inner, _) => {
                            let inner_size = self.get_type_size(inner);
                            self.lower_init_list_to_stores(dest_var, nested_items, inner, inner_size, bid)?;
                        }
                        Type::Struct(_) | Type::Union(_) => {
                            self.lower_struct_init_list(dest_var, elem_type, nested_items, bid)?;
                        }
                        _ => return Err(format!("Nested init list for non-compound type {:?}", elem_type)),
                    }
                }
                _ => {
                    let val = self.lower_expr(&item.value)?;
                    self.blocks[bid.0].instructions.push(Instruction::Store {
                        addr: Operand::Var(dest_var),
                        src: val,
                        value_type: elem_type.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Lower a struct/union initializer list to a sequence of GEP+Store instructions.
    /// `base_var` is the alloca'd struct address.
    pub(crate) fn lower_struct_init_list(
        &mut self,
        base_var: VarId,
        struct_type: &Type,
        items: &[model::InitItem],
        bid: BlockId,
    ) -> Result<(), String> {
        let type_name = match struct_type {
            Type::Struct(name) => name.clone(),
            Type::Union(name) => name.clone(),
            _ => return Err(format!("Expected struct/union type, got {:?}", struct_type)),
        };

        let is_union = matches!(struct_type, Type::Union(_));
        let fields: Vec<model::StructField> = if let Some(s_def) = self.struct_defs.get(&type_name).cloned() {
            s_def.fields.clone()
        } else if let Some(u_def) = self.union_defs.get(&type_name).cloned() {
            u_def.fields.clone()
        } else {
            return Err(format!("Unknown struct/union type '{}'", type_name));
        };

        let mut field_idx = 0usize;
        for item in items {
            // Determine which field to initialize
            let target_idx = match &item.designator {
                Some(model::Designator::Field(name)) => {
                    fields.iter().position(|f| &f.name == name)
                        .ok_or_else(|| format!("No field '{}' in struct '{}'", name, type_name))?
                }
                Some(model::Designator::Index(_)) => {
                    return Err("Index designator not valid in struct initializer".to_string());
                }
                None => {
                    let idx = field_idx;
                    idx
                }
            };
            field_idx = target_idx + 1;

            let field = &fields[target_idx];

            // For unions, all fields start at offset 0
            let (offset, field_type) = if is_union {
                (0i64, field.field_type.clone())
            } else {
                self.get_member_offset(&type_name, &field.name)
            };

            let dest_var = if offset == 0 {
                base_var
            } else {
                let offset_var = self.new_var();
                self.blocks[bid.0].instructions.push(Instruction::Binary {
                    dest: offset_var,
                    op: BinaryOp::Add,
                    left: Operand::Var(base_var),
                    right: Operand::Constant(offset),
                });
                offset_var
            };

            match &item.value {
                AstExpr::InitList(nested_items) => {
                    match &field_type {
                        Type::Array(inner, _) => {
                            let inner_size = self.get_type_size(inner);
                            self.lower_init_list_to_stores(dest_var, nested_items, inner, inner_size, bid)?;
                        }
                        Type::Struct(_) | Type::Union(_) => {
                            self.lower_struct_init_list(dest_var, &field_type, nested_items, bid)?;
                        }
                        _ => return Err(format!("Nested init list for non-compound field type {:?}", field_type)),
                    }
                }
                _ => {
                    let val = self.lower_expr(&item.value)?;
                    self.blocks[bid.0].instructions.push(Instruction::Store {
                        addr: Operand::Var(dest_var),
                        src: val,
                        value_type: field_type.clone(),
                    });
                }
            }

            // For unions, only initialize the first field
            if is_union {
                break;
            }
        }
        Ok(())
    }
}
