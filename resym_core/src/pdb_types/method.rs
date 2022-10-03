use super::{
    argument_list, field::FieldAccess, primitive_types::PrimitiveReconstructionFlavor, type_name,
    TypeForwarder, TypeSet,
};
use crate::error::{Result, ResymCoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method<'p> {
    pub name: pdb::RawString<'p>,
    pub return_type_name: (String, String),
    pub arguments: Vec<String>,
    pub is_virtual: bool,
    pub is_pure_virtual: bool,
    pub is_ctor: bool,
    pub is_dtor: bool,
    pub is_const: bool,
    pub is_volatile: bool,
    pub access: FieldAccess,
}

impl<'p> Method<'p> {
    pub fn find(
        name: pdb::RawString<'p>,
        attributes: pdb::FieldAttributes,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        type_index: pdb::TypeIndex,
        primitive_flavor: &PrimitiveReconstructionFlavor,
        needed_types: &mut TypeSet,
    ) -> Result<Method<'p>> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::MemberFunction(data) => Ok(Method {
                name,
                return_type_name: type_name(
                    type_finder,
                    type_forwarder,
                    data.return_type,
                    primitive_flavor,
                    needed_types,
                )?,
                arguments: argument_list(
                    type_finder,
                    type_forwarder,
                    data.argument_list,
                    primitive_flavor,
                    needed_types,
                )?,
                is_virtual: attributes.is_virtual()
                    | attributes.is_pure_virtual()
                    | attributes.is_intro_virtual(),
                // FIXME: Check the `is_intro_virtual` issue.
                is_pure_virtual: attributes.is_pure_virtual(),
                is_ctor: data.attributes.is_constructor()
                    || data.attributes.is_constructor_with_virtual_bases(),
                is_dtor: name.to_string().starts_with('~'),
                is_const: {
                    if let Some(func_modifier) = Method::find_func_modifier(&data, type_finder) {
                        func_modifier.constant
                    } else {
                        false
                    }
                },
                is_volatile: {
                    if let Some(func_modifier) = Method::find_func_modifier(&data, type_finder) {
                        func_modifier.volatile
                    } else {
                        false
                    }
                },
                access: FieldAccess::from_field_attribute(attributes.access()),
            }),

            other => {
                log::error!("other: {:?}", other);
                Err(ResymCoreError::NotImplementedError(
                    "Unhandled type data".to_owned(),
                ))
            }
        }
    }

    pub fn find_func_modifier(
        member_func_type: &pdb::MemberFunctionType,
        type_finder: &pdb::TypeFinder<'p>,
    ) -> Option<pdb::ModifierType> {
        if let Some(this_pointer_type) = member_func_type.this_pointer_type {
            match type_finder.find(this_pointer_type).ok()?.parse().ok()? {
                pdb::TypeData::Pointer(data) => {
                    match type_finder.find(data.underlying_type).ok()?.parse().ok()? {
                        pdb::TypeData::Modifier(data) => Some(data),
                        _ => {
                            // no modifier on this_pointer_type
                            None
                        }
                    }
                }
                _ => {
                    // this_pointer_type is not a pointer
                    None
                }
            }
        } else {
            // no this pointer
            None
        }
    }
}
