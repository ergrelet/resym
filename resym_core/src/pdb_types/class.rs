use std::fmt;

use super::{
    enumeration::Enum,
    field::{FieldAccess, StaticField},
    fmt_struct_fields_recursive, is_unnamed_type,
    primitive_types::PrimitiveReconstructionFlavor,
    resolve_complete_type_index, type_bitfield_info, type_name, type_size,
    union::Union,
    DataFormatConfiguration, Field, Method, Result, ResymCoreError, TypeForwarder, TypeSet,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassAccess {
    None,
    Private,
    Protected,
    Public,
}
impl ClassAccess {
    pub fn from_field_attribute(value: u8) -> Self {
        match value {
            0 => ClassAccess::None,
            1 => ClassAccess::Private,
            2 => ClassAccess::Protected,
            3 => ClassAccess::Public,
            _ => unreachable!("Major PDB format update?"),
        }
    }
}
impl fmt::Display for ClassAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ClassAccess::None => "",
                ClassAccess::Private => "private",
                ClassAccess::Protected => "protected",
                ClassAccess::Public => "public",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseClass {
    type_name: String,
    offset: u32,
    access: ClassAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class<'p> {
    pub kind: pdb::ClassKind,
    pub name: String,
    pub size: u64,
    pub base_classes: Vec<BaseClass>,
    pub fields: Vec<Field<'p>>,
    pub static_fields: Vec<StaticField<'p>>,
    pub instance_methods: Vec<Method<'p>>,
    pub static_methods: Vec<Method<'p>>,
    pub nested_classes: Vec<Class<'p>>,
    pub nested_unions: Vec<Union<'p>>,
    pub nested_enums: Vec<Enum<'p>>,
}

impl<'p> Class<'p> {
    #[allow(clippy::unnecessary_wraps)]
    pub fn add_derived_from(
        &mut self,
        _: &pdb::TypeFinder<'p>,
        _: pdb::TypeIndex,
        _: &mut TypeSet,
    ) -> Result<()> {
        // TODO
        Ok(())
    }

    pub fn add_fields(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        type_index: pdb::TypeIndex,
        primitive_flavor: &PrimitiveReconstructionFlavor,
        needed_types: &mut TypeSet,
    ) -> Result<()> {
        // Resolve the complete type's index, if present in the PDB
        let complete_type_index = resolve_complete_type_index(type_forwarder, type_index);
        match type_finder.find(complete_type_index)?.parse()? {
            pdb::TypeData::FieldList(data) => {
                for field in &data.fields {
                    self.add_field(
                        type_finder,
                        type_forwarder,
                        field,
                        primitive_flavor,
                        needed_types,
                    )?;
                }

                if let Some(continuation) = data.continuation {
                    // recurse
                    self.add_fields(
                        type_finder,
                        type_forwarder,
                        continuation,
                        primitive_flavor,
                        needed_types,
                    )?;
                }
            }

            // Nested types
            pdb::TypeData::Class(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                let mut class = Class {
                    kind: data.kind,
                    name,
                    size: data.size,
                    fields: Vec::new(),
                    static_fields: Vec::new(),
                    base_classes: Vec::new(),
                    instance_methods: Vec::new(),
                    static_methods: Vec::new(),
                    nested_classes: Vec::new(),
                    nested_unions: Vec::new(),
                    nested_enums: Vec::new(),
                };

                if let Some(derived_from) = data.derived_from {
                    class.add_derived_from(type_finder, derived_from, needed_types)?;
                }

                if let Some(fields) = data.fields {
                    class.add_fields(
                        type_finder,
                        type_forwarder,
                        fields,
                        primitive_flavor,
                        needed_types,
                    )?;
                }

                self.nested_classes.insert(0, class);
            }

            pdb::TypeData::Union(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                let mut u = Union {
                    name,
                    size: data.size,
                    fields: Vec::new(),
                    static_fields: Vec::new(),
                    instance_methods: Vec::new(),
                    static_methods: Vec::new(),
                    nested_classes: Vec::new(),
                    nested_unions: Vec::new(),
                    nested_enums: Vec::new(),
                };

                u.add_fields(
                    type_finder,
                    type_forwarder,
                    data.fields,
                    primitive_flavor,
                    needed_types,
                )?;

                self.nested_unions.insert(0, u);
            }

            pdb::TypeData::Enumeration(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                let mut e = Enum {
                    name,
                    underlying_type_name: type_name(
                        type_finder,
                        type_forwarder,
                        data.underlying_type,
                        primitive_flavor,
                        needed_types,
                    )?
                    .0,
                    values: Vec::new(),
                };

                e.add_fields(type_finder, data.fields, needed_types)?;

                self.nested_enums.insert(0, e);
            }

            pdb::TypeData::Primitive(_)
            | pdb::TypeData::Pointer(_)
            | pdb::TypeData::Procedure(_)
            | pdb::TypeData::Modifier(_) => {
                // TODO: What does this represent? Suppress warnings for the
                // moment
            }

            other => {
                log::warn!(
                    "trying to Class::add_fields() got {} -> {:?}",
                    type_index,
                    other
                );
            }
        }

        Ok(())
    }

    fn add_field(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        field: &pdb::TypeData<'p>,
        primitive_flavor: &PrimitiveReconstructionFlavor,
        needed_types: &mut TypeSet,
    ) -> Result<()> {
        match *field {
            pdb::TypeData::Member(ref data) => {
                // Resolve the complete type's index, if present in the PDB
                let complete_type_index =
                    resolve_complete_type_index(type_forwarder, data.field_type);
                let (type_left, type_right) = type_name(
                    type_finder,
                    type_forwarder,
                    complete_type_index,
                    primitive_flavor,
                    needed_types,
                )?;
                let type_bitfield_info = type_bitfield_info(type_finder, complete_type_index)?;
                let type_size = type_size(type_finder, complete_type_index)?;
                let access = FieldAccess::from_field_attribute(data.attributes.access());

                self.fields.push(Field {
                    type_left,
                    type_right,
                    name: data.name,
                    offset: data.offset,
                    size: type_size,
                    bitfield_info: type_bitfield_info,
                    access,
                });
            }

            pdb::TypeData::StaticMember(ref data) => {
                // Resolve the complete type's index, if present in the PDB
                let complete_type_index =
                    resolve_complete_type_index(type_forwarder, data.field_type);
                let (type_left, type_right) = type_name(
                    type_finder,
                    type_forwarder,
                    complete_type_index,
                    primitive_flavor,
                    needed_types,
                )?;
                let access = FieldAccess::from_field_attribute(data.attributes.access());

                self.static_fields.push(StaticField {
                    type_left,
                    type_right,
                    name: data.name,
                    access,
                });
            }

            pdb::TypeData::Method(ref data) => {
                let method = Method::find(
                    data.name,
                    data.attributes,
                    type_finder,
                    type_forwarder,
                    data.method_type,
                    primitive_flavor,
                    needed_types,
                )?;
                if data.attributes.is_static() {
                    self.static_methods.push(method);
                } else {
                    self.instance_methods.push(method);
                }
            }

            pdb::TypeData::OverloadedMethod(ref data) => {
                // this just means we have more than one method with the same name
                // find the method list
                match type_finder.find(data.method_list)?.parse()? {
                    pdb::TypeData::MethodList(method_list) => {
                        for pdb::MethodListEntry {
                            attributes,
                            method_type,
                            ..
                        } in method_list.methods
                        {
                            // hooray
                            let method = Method::find(
                                data.name,
                                attributes,
                                type_finder,
                                type_forwarder,
                                method_type,
                                primitive_flavor,
                                needed_types,
                            )?;

                            if attributes.is_static() {
                                self.static_methods.push(method);
                            } else {
                                self.instance_methods.push(method);
                            }
                        }
                    }
                    other => {
                        log::error!(
                            "processing OverloadedMethod, expected MethodList, got {} -> {:?}",
                            data.method_list,
                            other
                        );
                        return Err(ResymCoreError::InvalidParameterError(
                            "unexpected type in Class::add_field()".to_string(),
                        ));
                    }
                }
            }

            pdb::TypeData::BaseClass(ref data) => {
                // Resolve the complete type's index, if present in the PDB
                let complete_base_class_type_index =
                    resolve_complete_type_index(type_forwarder, data.base_class);
                self.base_classes.push(BaseClass {
                    type_name: type_name(
                        type_finder,
                        type_forwarder,
                        complete_base_class_type_index,
                        primitive_flavor,
                        needed_types,
                    )?
                    .0,
                    offset: data.offset,
                    access: ClassAccess::from_field_attribute(data.attributes.access()),
                })
            }

            pdb::TypeData::VirtualBaseClass(ref data) => {
                // Resolve the complete type's index, if present in the PDB
                let complete_base_class_type_index =
                    resolve_complete_type_index(type_forwarder, data.base_class);
                self.base_classes.push(BaseClass {
                    type_name: type_name(
                        type_finder,
                        type_forwarder,
                        complete_base_class_type_index,
                        primitive_flavor,
                        needed_types,
                    )?
                    .0,
                    offset: data.base_pointer_offset,
                    access: ClassAccess::from_field_attribute(data.attributes.access()),
                })
            }

            pdb::TypeData::VirtualFunctionTablePointer(ref _data) => {
                // TODO: Display a comment at the beginning of the declaration
                // to make it obvious a vtable is present?
            }

            // Nested type declaration
            pdb::TypeData::Nested(ref _data) => {
                // TODO: Properly handle nested types
                // let complete_type_index =
                //     resolve_complete_type_index(type_forwarder, data.nested_type);
                // self.add_fields(
                //     type_finder,
                //     type_forwarder,
                //     complete_type_index,
                //     needed_types,
                // )?;
            }

            ref other => {
                log::error!("trying to Class::add_field(): {:?}", other);
                return Err(ResymCoreError::InvalidParameterError(
                    "unexpected type in Class::add_field()".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn reconstruct(
        &self,
        fmt_configuration: &DataFormatConfiguration,
        f: &mut impl std::fmt::Write,
    ) -> fmt::Result {
        write!(
            f,
            "{} {}",
            match self.kind {
                pdb::ClassKind::Class => "class",
                pdb::ClassKind::Struct => "struct",
                // Not used C and C++ but well ...
                pdb::ClassKind::Interface => "interface",
            },
            self.name
        )?;

        if !self.base_classes.is_empty() {
            for (i, base) in self.base_classes.iter().enumerate() {
                let prefix = match i {
                    0 => " :",
                    _ => ",",
                };
                write!(f, "{} {} {}", prefix, base.access, base.type_name)?;
            }
        }

        writeln!(f, " {{ /* Size={:#x} */", self.size)?;

        for base in &self.base_classes {
            writeln!(
                f,
                "  /* {:#06x}: fields for {} */",
                base.offset, base.type_name
            )?;
        }

        // Nested declarations
        if !self.nested_classes.is_empty() {
            writeln!(f, "  ")?;
            for class in &self.nested_classes {
                class.reconstruct(fmt_configuration, f)?;
            }
        }
        if !self.nested_unions.is_empty() {
            writeln!(f, "  ")?;
            for u in &self.nested_unions {
                u.reconstruct(fmt_configuration, f)?;
            }
        }
        if !self.nested_enums.is_empty() {
            writeln!(f, "  ")?;
            for e in &self.nested_enums {
                e.reconstruct(f)?;
            }
        }

        // Dump fields while detecting unnamed structs and unions
        fmt_struct_fields_recursive(fmt_configuration, &self.fields, 1, f)?;

        // Static fields
        for field in &self.static_fields {
            writeln!(
                f,
                "  {}static {} {}{};",
                if fmt_configuration.print_access_specifiers {
                    &field.access
                } else {
                    &FieldAccess::None
                },
                field.type_left,
                &field.name,
                field.type_right,
            )?;
        }

        if !self.instance_methods.is_empty() {
            writeln!(f, "  ")?;
            for method in &self.instance_methods {
                writeln!(
                    f,
                    "  {}{}{}{}{}({}){}{}{}{};",
                    if fmt_configuration.print_access_specifiers {
                        &method.access
                    } else {
                        &FieldAccess::None
                    },
                    if method.is_virtual { "virtual " } else { "" },
                    if method.is_ctor || method.is_dtor {
                        ""
                    } else {
                        &method.return_type_name.0
                    },
                    if !method.is_ctor && !method.is_dtor && method.return_type_name.1.is_empty() {
                        " "
                    } else {
                        ""
                    },
                    &method.name,
                    method.arguments.join(", "),
                    method.return_type_name.1,
                    if method.is_const { " const" } else { "" },
                    if method.is_volatile { " volatile" } else { "" },
                    if method.is_pure_virtual { " = 0" } else { "" },
                )?;
            }
        }

        if !self.static_methods.is_empty() {
            writeln!(f, "  ")?;
            for method in &self.static_methods {
                writeln!(
                    f,
                    "  {}static {}{}{}({}){}{}{};",
                    if fmt_configuration.print_access_specifiers {
                        &method.access
                    } else {
                        &FieldAccess::None
                    },
                    method.return_type_name.0,
                    if method.return_type_name.1.is_empty() {
                        " "
                    } else {
                        ""
                    },
                    &method.name,
                    method.arguments.join(", "),
                    method.return_type_name.1,
                    if method.is_const { " const" } else { "" },
                    if method.is_volatile { " volatile" } else { "" },
                )?;
            }
        }

        writeln!(f, "}};")?;

        Ok(())
    }
}
