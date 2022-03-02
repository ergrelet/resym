use anyhow::{anyhow, Result};

use super::{argument_list, field::FieldAccess, type_name, TypeForwarder, TypeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method<'p> {
    pub name: pdb::RawString<'p>,
    pub return_type_name: String,
    pub arguments: Vec<String>,
    pub is_virtual: bool,
    pub is_pure_virtual: bool,
    pub is_ctor: bool,
    pub access: FieldAccess,
}

impl<'p> Method<'p> {
    pub fn find(
        name: pdb::RawString<'p>,
        attributes: pdb::FieldAttributes,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        type_index: pdb::TypeIndex,
        needed_types: &mut TypeSet,
    ) -> Result<Method<'p>> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::MemberFunction(data) => Ok(Method {
                name,
                return_type_name: type_name(
                    type_finder,
                    type_forwarder,
                    data.return_type,
                    needed_types,
                )?
                .0,
                arguments: argument_list(
                    type_finder,
                    type_forwarder,
                    data.argument_list,
                    needed_types,
                )?,
                is_virtual: attributes.is_virtual()
                    | attributes.is_pure_virtual()
                    | attributes.is_intro_virtual(),
                // FIXME: Check the `is_intro_virtual` issue.
                is_pure_virtual: attributes.is_pure_virtual(),
                is_ctor: data.attributes.is_constructor()
                    || data.attributes.is_constructor_with_virtual_bases(),
                access: FieldAccess::from_field_attribute(attributes.access()),
            }),

            other => {
                log::error!("other: {:?}", other);
                Err(anyhow!("Unhandled type data"))
            }
        }
    }
}
