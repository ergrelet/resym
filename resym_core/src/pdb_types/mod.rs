mod class;
mod enumeration;
mod field;
mod forward_declaration;
mod forward_reference;
mod method;
mod primitive_types;
mod union;

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::ops::Range;

use crate::error::{Result, ResymCoreError};
use class::Class;
use enumeration::Enum;
use field::{Field, FieldAccess};
use method::Method;
use primitive_types::primitive_kind_as_str;
use union::Union;

pub use primitive_types::{include_headers_for_flavor, PrimitiveReconstructionFlavor};

use self::forward_declaration::{ForwardDeclaration, ForwardDeclarationKind};
use self::forward_reference::ForwardReference;

/// Set of (`TypeIndex`, bool) tuples.
///
/// The boolean value indicates whether the type was referenced via a pointer
/// or a C++ reference.
pub type NeededTypeSet = HashSet<(pdb::TypeIndex, bool)>;

pub type TypeForwarder = dashmap::DashMap<pdb::TypeIndex, pdb::TypeIndex>;

/// Return a pair of strings representing the given `type_index`.
pub fn type_name(
    type_finder: &pdb::TypeFinder,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut NeededTypeSet,
) -> Result<(String, String)> {
    let (type_left, type_right) = match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Primitive(data) => {
            let name =
                primitive_kind_as_str(primitive_flavor, data.kind, data.indirection.is_some())?;

            (name, String::default())
        }

        pdb::TypeData::Class(data) => {
            needed_types.insert((type_index, false));
            // Rename unnamed anonymous tags to something unique
            let name = data.name.to_string();
            if is_unnamed_type(&name) {
                let name = format!("_unnamed_{type_index}");
                (name, String::default())
            } else {
                (name.into_owned(), String::default())
            }
        }

        pdb::TypeData::Union(data) => {
            needed_types.insert((type_index, false));
            // Rename unnamed anonymous tags to something unique
            let name = data.name.to_string();
            if is_unnamed_type(&name) {
                let name = format!("_unnamed_{type_index}");
                (name, String::default())
            } else {
                (name.into_owned(), String::default())
            }
        }

        pdb::TypeData::Enumeration(data) => {
            needed_types.insert((type_index, false));
            (data.name.to_string().into_owned(), String::default())
        }

        pdb::TypeData::Pointer(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_underlying_type_index =
                resolve_complete_type_index(type_forwarder, data.underlying_type);
            let mut temporary_needed_types = HashSet::new();
            let (type_left, type_right) = type_name(
                type_finder,
                type_forwarder,
                complete_underlying_type_index,
                primitive_flavor,
                &mut temporary_needed_types,
            )?;

            if temporary_needed_types.len() < 2 {
                // "Simple" type (e.g., class, union, enum) -> add as pointer
                if let Some(needed_type) = temporary_needed_types.into_iter().next() {
                    needed_types.insert((needed_type.0, true));
                }
            } else {
                // "Complex" type (e.g., procedure) -> add as is
                needed_types.extend(temporary_needed_types);
            }

            if data.attributes.is_reference() {
                (format!("{type_left}&"), type_right)
            } else {
                (format!("{type_left}*"), type_right)
            }
        }

        pdb::TypeData::Modifier(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_underlying_type_index =
                resolve_complete_type_index(type_forwarder, data.underlying_type);
            let (type_left, type_right) = type_name(
                type_finder,
                type_forwarder,
                complete_underlying_type_index,
                primitive_flavor,
                needed_types,
            )?;

            if data.constant {
                (format!("const {type_left}"), type_right)
            } else if data.volatile {
                (format!("volatile {type_left}"), type_right)
            } else {
                // ?
                (type_left, type_right)
            }
        }

        pdb::TypeData::Array(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_element_type_index =
                resolve_complete_type_index(type_forwarder, data.element_type);
            let ((type_left, type_right), mut dimensions) = array_base_name(
                type_finder,
                type_forwarder,
                complete_element_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let type_size = u32::try_from(type_size(type_finder, complete_element_type_index)?)?;
            let mut divider = if type_size == 0 {
                log::warn!(
                    "'{}{}' has invalid size (0), array dimensions might be incorrect",
                    type_left,
                    type_right,
                );
                1
            } else {
                type_size
            };

            let mut dimensions_elem_count = data
                .dimensions
                .into_iter()
                .map(|dim_size| {
                    let result = dim_size / divider;
                    divider = dim_size;
                    result as usize
                })
                .collect::<Vec<_>>();
            dimensions.append(&mut dimensions_elem_count);

            let mut dimensions_str = String::default();
            // Note: Dimensions are collected in reverse order so we have to use
            // a reverse iterator
            for dim in dimensions.iter().rev() {
                dimensions_str = format!("{dimensions_str}[{dim}]");
            }

            (type_left, format!("{}{}", dimensions_str, type_right))
        }

        pdb::TypeData::Bitfield(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_underlying_type_index =
                resolve_complete_type_index(type_forwarder, data.underlying_type);
            let (type_left, type_right) = type_name(
                type_finder,
                type_forwarder,
                complete_underlying_type_index,
                primitive_flavor,
                needed_types,
            )?;
            (type_left, format!("{} : {}", type_right, data.length))
        }

        pdb::TypeData::Procedure(data) => {
            // TODO: Parse and display attributes
            let (ret_type_left, ret_type_right) = if let Some(return_type) = data.return_type {
                // Resolve the complete type's index, if present in the PDB
                let complete_return_type_index =
                    resolve_complete_type_index(type_forwarder, return_type);
                type_name(
                    type_finder,
                    type_forwarder,
                    complete_return_type_index,
                    primitive_flavor,
                    needed_types,
                )?
            } else {
                ("void".to_string(), String::default())
            };
            let arg_list = argument_list(
                type_finder,
                type_forwarder,
                data.argument_list,
                primitive_flavor,
                needed_types,
            )?;

            (
                format!("{ret_type_left}{ret_type_right} ("),
                format!(
                    ")({})",
                    arg_list
                        .into_iter()
                        .map(|(type_left, type_right)| format!("{type_left}{type_right}"))
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
            )
        }

        pdb::TypeData::MemberFunction(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_return_type_index =
                resolve_complete_type_index(type_forwarder, data.return_type);
            let complete_class_type_index =
                resolve_complete_type_index(type_forwarder, data.class_type);
            // // TODO: Parse and display attributes
            let (ret_type_left, ret_type_right) = type_name(
                type_finder,
                type_forwarder,
                complete_return_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let (class_type_left, _) = type_name(
                type_finder,
                type_forwarder,
                complete_class_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let arg_list = argument_list(
                type_finder,
                type_forwarder,
                data.argument_list,
                primitive_flavor,
                needed_types,
            )?;

            (
                format!("{ret_type_left}{ret_type_right} ({class_type_left}::"),
                format!(
                    ")({})",
                    arg_list
                        .into_iter()
                        .map(|(type_left, type_right)| format!("{type_left}{type_right}"))
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
            )
        }

        type_data => {
            log::warn!(
                "FIXME: figure out how to name it: TypeIndex={}, TypeData={:?}",
                type_index,
                type_data
            );
            ("FIXME_UNKNOWN_TYPE".to_string(), String::default())
        }
    };

    // TODO: search and replace std:: patterns (see issue #4)

    Ok((type_left, type_right))
}

fn array_base_name(
    type_finder: &pdb::TypeFinder,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut NeededTypeSet,
) -> Result<((String, String), Vec<usize>)> {
    match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Array(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_element_type_index =
                resolve_complete_type_index(type_forwarder, data.element_type);
            let ((type_left, type_right), mut base_dimensions) = array_base_name(
                type_finder,
                type_forwarder,
                complete_element_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let type_size = u32::try_from(type_size(type_finder, complete_element_type_index)?)?;
            let mut divider = if type_size == 0 {
                log::warn!(
                    "'{}{}' has invalid size (0), array dimensions might be incorrect",
                    type_left,
                    type_right,
                );
                1
            } else {
                type_size
            };

            let mut dimensions_elem_count = data
                .dimensions
                .into_iter()
                .map(|dim_size| {
                    let result = dim_size / divider;
                    divider = dim_size;
                    result as usize
                })
                .collect::<Vec<_>>();
            base_dimensions.append(&mut dimensions_elem_count);

            Ok(((type_left, type_right), base_dimensions))
        }
        _ => Ok((
            type_name(
                type_finder,
                type_forwarder,
                type_index,
                primitive_flavor,
                needed_types,
            )?,
            vec![],
        )),
    }
}

pub fn argument_list(
    type_finder: &pdb::TypeFinder,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut NeededTypeSet,
) -> Result<Vec<(String, String)>> {
    match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::ArgumentList(data) => {
            let mut args = Vec::new();
            for arg_type in data.arguments {
                args.push(type_name(
                    type_finder,
                    type_forwarder,
                    arg_type,
                    primitive_flavor,
                    needed_types,
                )?);
            }
            Ok(args)
        }
        _ => Err(ResymCoreError::InvalidParameterError(
            "argument list of non-argument-list type".to_owned(),
        )),
    }
}

/// Return the type's offset in bits, if the type is a bitfield.
pub fn type_bitfield_info(
    type_finder: &pdb::TypeFinder,
    type_index: pdb::TypeIndex,
) -> Result<Option<(u8, u8)>> {
    let bitfield_info = match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Bitfield(data) => Some((data.position, data.length)),
        _ => None,
    };

    Ok(bitfield_info)
}

/// Return the type's size in bytes.
pub fn type_size(type_finder: &pdb::TypeFinder, type_index: pdb::TypeIndex) -> Result<usize> {
    let size = match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Primitive(data) => {
            let mut size = match data.kind {
                pdb::PrimitiveKind::Char
                | pdb::PrimitiveKind::RChar
                | pdb::PrimitiveKind::UChar
                | pdb::PrimitiveKind::I8
                | pdb::PrimitiveKind::U8
                | pdb::PrimitiveKind::Bool8 => 1,

                pdb::PrimitiveKind::WChar
                | pdb::PrimitiveKind::RChar16
                | pdb::PrimitiveKind::I16
                | pdb::PrimitiveKind::Short
                | pdb::PrimitiveKind::U16
                | pdb::PrimitiveKind::UShort
                | pdb::PrimitiveKind::Bool16 => 2,

                pdb::PrimitiveKind::RChar32
                | pdb::PrimitiveKind::I32
                | pdb::PrimitiveKind::Long
                | pdb::PrimitiveKind::U32
                | pdb::PrimitiveKind::ULong
                | pdb::PrimitiveKind::F32
                | pdb::PrimitiveKind::Bool32 => 4,

                pdb::PrimitiveKind::I64
                | pdb::PrimitiveKind::Quad
                | pdb::PrimitiveKind::U64
                | pdb::PrimitiveKind::UQuad
                | pdb::PrimitiveKind::F64
                | pdb::PrimitiveKind::Bool64 => 8,

                _ => 0,
            };

            if let Some(indirection) = data.indirection {
                size = match indirection {
                    pdb::Indirection::Near16
                    | pdb::Indirection::Far16
                    | pdb::Indirection::Huge16 => 2,
                    pdb::Indirection::Near32 | pdb::Indirection::Far32 => 4,
                    pdb::Indirection::Near64 => 8,
                    pdb::Indirection::Near128 => 16,
                };
            }

            size
        }

        pdb::TypeData::Class(data) => data.size as usize,

        pdb::TypeData::Enumeration(data) => type_size(type_finder, data.underlying_type)?,

        pdb::TypeData::Union(data) => data.size as usize,

        pdb::TypeData::Pointer(data) => match data.attributes.pointer_kind() {
            pdb::PointerKind::Near16 | pdb::PointerKind::Far16 | pdb::PointerKind::Huge16 => 2,

            pdb::PointerKind::Near32
            | pdb::PointerKind::Far32
            | pdb::PointerKind::BaseSeg
            | pdb::PointerKind::BaseVal
            | pdb::PointerKind::BaseSegVal
            | pdb::PointerKind::BaseAddr
            | pdb::PointerKind::BaseSegAddr
            | pdb::PointerKind::BaseType
            | pdb::PointerKind::BaseSelf => 4,

            pdb::PointerKind::Ptr64 => 8,
        },

        pdb::TypeData::Modifier(data) => type_size(type_finder, data.underlying_type)?,

        pdb::TypeData::Array(data) => *data.dimensions.iter().last().unwrap_or(&0) as usize,

        _ => 0,
    };

    Ok(size)
}

/// Indicate if the given `type_name` is the name of an anonymous type.
pub fn is_unnamed_type(type_name: &str) -> bool {
    type_name.contains("<anonymous-")
        || type_name.contains("<unnamed-")
        || type_name.contains("__unnamed")
}

/// Trait for type data that can be reconstructed to C++
pub trait ReconstructibleTypeData {
    fn reconstruct(
        &self,
        fmt_configuration: &DataFormatConfiguration,
        f: &mut impl std::fmt::Write,
    ) -> fmt::Result;
}

/// Struct that represent a set of reconstructed types (forward declarations,
/// classes/structs, enums and unions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Data<'p> {
    /// Forward-declared types which are referenced in this PDB but not defined in it
    forward_references: BTreeMap<pdb::TypeIndex, ForwardReference>,
    /// Forward-declared types which are defined in this PDB
    forward_declarations: BTreeMap<pdb::TypeIndex, ForwardDeclaration>,
    /// Enum types
    enums: BTreeMap<pdb::TypeIndex, Enum<'p>>,
    /// Class/struct types
    classes: BTreeMap<pdb::TypeIndex, Class<'p>>,
    /// Union types
    unions: BTreeMap<pdb::TypeIndex, Union<'p>>,
    /// Unique type names
    type_names: HashSet<String>,
}

impl Data<'_> {
    pub fn reconstruct(
        &self,
        fmt_configuration: &DataFormatConfiguration,
        type_depth_map: &BTreeMap<usize, Vec<pdb::TypeIndex>>,
        output_writer: &mut impl std::fmt::Write,
    ) -> Result<()> {
        // Types without definition
        if !self.forward_references.is_empty() {
            writeln!(output_writer)?;
        }
        for e in self.forward_references.values() {
            e.reconstruct(fmt_configuration, output_writer)?;
        }

        // Forward declarations
        if !self.forward_declarations.is_empty() {
            writeln!(output_writer)?;
        }
        for e in self.forward_declarations.values() {
            e.reconstruct(fmt_configuration, output_writer)?;
        }

        if !type_depth_map.is_empty() {
            // Follow type depth map order
            for type_indices in type_depth_map.values().rev() {
                for type_index in type_indices.iter() {
                    // Enum definitions
                    if let Some(e) = self.enums.get(type_index) {
                        writeln!(output_writer)?;
                        e.reconstruct(fmt_configuration, output_writer)?;
                    }
                    // Class definitions
                    else if let Some(c) = self.classes.get(type_index) {
                        writeln!(output_writer)?;
                        c.reconstruct(fmt_configuration, output_writer)?;
                    }
                    // Union definitions
                    else if let Some(u) = self.unions.get(type_index) {
                        writeln!(output_writer)?;
                        u.reconstruct(fmt_configuration, output_writer)?;
                    }
                }
            }
        } else {
            // Follow type index order
            //
            // Enum definitions
            for e in self.enums.values() {
                writeln!(output_writer)?;
                e.reconstruct(fmt_configuration, output_writer)?;
            }

            // Class/struct definitions
            for class in self.classes.values() {
                writeln!(output_writer)?;
                class.reconstruct(fmt_configuration, output_writer)?;
            }

            // Union definitions
            for u in self.unions.values() {
                writeln!(output_writer)?;
                u.reconstruct(fmt_configuration, output_writer)?;
            }
        }

        Ok(())
    }
}

impl<'p> Default for Data<'p> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'p> Data<'p> {
    pub fn new() -> Self {
        Self {
            forward_references: BTreeMap::new(),
            forward_declarations: BTreeMap::new(),
            classes: BTreeMap::new(),
            enums: BTreeMap::new(),
            unions: BTreeMap::new(),
            type_names: HashSet::new(),
        }
    }

    pub fn add(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        type_index: pdb::TypeIndex,
        primitive_flavor: &PrimitiveReconstructionFlavor,
        needed_types: &mut NeededTypeSet,
    ) -> Result<()> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::Class(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                if self.type_names.contains(&name) {
                    // Type has already been added, return
                    return Ok(());
                }

                if data.properties.forward_reference() {
                    self.type_names.insert(name.clone());
                    self.forward_references.insert(
                        type_index,
                        ForwardReference {
                            index: type_index,
                            kind: data.kind,
                            name,
                        },
                    );

                    return Ok(());
                }

                let mut class = Class {
                    index: type_index,
                    kind: data.kind,
                    name: name.clone(),
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
                    // Note: Do not propagate the error, this allows the
                    // reconstruction to go through even when LF_TYPESERVER_ST
                    // types are encountered. Alert the user that the result
                    // might be incomplete.
                    if let Err(err) = class.add_fields(
                        type_finder,
                        type_forwarder,
                        fields,
                        primitive_flavor,
                        needed_types,
                    ) {
                        log::error!(
                            "Error encountered while reconstructing '{}': {}",
                            class.name,
                            err
                        );
                    }
                }

                self.type_names.insert(name);
                self.classes.insert(type_index, class);
            }

            pdb::TypeData::Union(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                if self.type_names.contains(&name) {
                    // Type has already been added, return
                    return Ok(());
                }

                let mut u = Union {
                    index: type_index,
                    name: name.clone(),
                    size: data.size,
                    fields: Vec::new(),
                    static_fields: Vec::new(),
                    instance_methods: Vec::new(),
                    static_methods: Vec::new(),
                    nested_classes: Vec::new(),
                    nested_unions: Vec::new(),
                    nested_enums: Vec::new(),
                };

                if let Err(err) = u.add_fields(
                    type_finder,
                    type_forwarder,
                    data.fields,
                    primitive_flavor,
                    needed_types,
                ) {
                    log::error!(
                        "Error encountered while reconstructing '{}': {}",
                        u.name,
                        err
                    );
                }

                self.type_names.insert(name);
                self.unions.insert(type_index, u);
            }

            pdb::TypeData::Enumeration(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                if self.type_names.contains(&name) {
                    // Type has already been added, return
                    return Ok(());
                }

                let mut e = Enum {
                    index: type_index,
                    name: name.clone(),
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

                if let Err(err) = e.add_fields(type_finder, data.fields, needed_types) {
                    log::error!(
                        "Error encountered while reconstructing '{}': {}",
                        e.name,
                        err
                    );
                }

                self.type_names.insert(name.clone());
                self.enums.insert(type_index, e);
            }

            // ignore
            other => log::debug!("don't know how to add {:?}", other),
        }

        Ok(())
    }

    pub fn add_as_forward_declaration(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_index: pdb::TypeIndex,
    ) -> Result<()> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::Class(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                self.forward_declarations.insert(
                    type_index,
                    ForwardDeclaration {
                        index: type_index,
                        kind: ForwardDeclarationKind::from_class_kind(data.kind),
                        name,
                    },
                );
            }

            pdb::TypeData::Union(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{type_index}")
                } else {
                    name_str.into_owned()
                };

                self.forward_declarations.insert(
                    type_index,
                    ForwardDeclaration {
                        index: type_index,
                        kind: ForwardDeclarationKind::Union,
                        name,
                    },
                );
            }

            _ => {}
        }

        Ok(())
    }
}

pub fn resolve_complete_type_index(
    forwarder_to_complete_type: &dashmap::DashMap<pdb::TypeIndex, pdb::TypeIndex>,
    type_index: pdb::TypeIndex,
) -> pdb::TypeIndex {
    match forwarder_to_complete_type.get(&type_index) {
        Some(d) => *d.value(),
        None => type_index,
    }
}

fn fmt_struct_fields_recursive(
    fmt_configuration: &DataFormatConfiguration,
    fields: &[Field],
    depth: usize,
    f: &mut impl std::fmt::Write,
) -> fmt::Result {
    if fields.is_empty() {
        return Ok(());
    }

    let unions_found = find_unnamed_unions_in_struct(fields);
    // Write fields into the `Formatter`
    let indentation = "  ".repeat(depth);
    let mut last_field: Option<&Field> = None;
    for union_range in unions_found {
        // Fields out of unnamed unions are represented by "empty" unions
        if union_range.is_empty() {
            let field = &fields[union_range.start];

            // Check if we need to add padding, following a bit-field member
            if let Some((field_bit_offset, _)) = field.bitfield_info {
                if let Some(last_field) = last_field {
                    if let Some((last_bit_offset, last_bit_size)) = last_field.bitfield_info {
                        let potential_padding_bit_offset = last_bit_offset + last_bit_size;
                        if field.offset == last_field.offset {
                            // Padding within the same allocation unit
                            let bit_offset_delta = field_bit_offset - potential_padding_bit_offset;
                            // Add padding if needed
                            if bit_offset_delta > 0 {
                                writeln!(
                                    f,
                                    "{}/* {:#06x} */ {} : {}; /* BitPos={} */",
                                    &indentation,
                                    last_field.offset,
                                    last_field.type_left,
                                    bit_offset_delta,
                                    potential_padding_bit_offset
                                )?;
                            }
                        } else {
                            // Padding in the previous field
                            // FIXME(ergrelet): 0-bit padding is used systematically when we should only emit it when
                            // needed. It's not incorrect but might produce less elegant output.
                            writeln!(
                                f,
                                "{}/* {:#06x} */ {} : 0; /* BitPos={} */",
                                &indentation,
                                last_field.offset,
                                last_field.type_left,
                                potential_padding_bit_offset
                            )?;
                        }
                    }
                }
            }

            writeln!(
                f,
                "{}/* {:#06x} */ {}{} {}{};{}",
                &indentation,
                field.offset,
                if fmt_configuration.print_access_specifiers {
                    &field.access
                } else {
                    &FieldAccess::None
                },
                field.type_left,
                field.name.to_string(),
                field.type_right,
                if let Some((bit_position, _)) = field.bitfield_info {
                    format!(" /* BitPos={bit_position} */")
                } else {
                    String::default()
                }
            )?;
            last_field = Some(field);
        } else {
            writeln!(f, "{}union {{", &indentation)?;
            fmt_union_fields_recursive(fmt_configuration, &fields[union_range], depth + 1, f)?;
            writeln!(f, "{}}};", &indentation)?;
            last_field = None;
        }
    }

    Ok(())
}

fn find_unnamed_unions_in_struct(fields: &[Field]) -> Vec<Range<usize>> {
    let mut unions_found: Vec<Range<usize>> = vec![];
    // Temporary map of unions and fields that'll be used to compute the list
    // of unnamed unions which are in the struct.
    let mut unions_found_temp: BTreeMap<(u64, u8), (Range<usize>, u64)> = BTreeMap::new();

    // Discover unions
    let mut curr_union_offset_range: Range<u64> = Range::default();
    for (i, field) in fields.iter().enumerate() {
        // Check if the field is located inside of the union we're processing
        if curr_union_offset_range.contains(&field.offset) {
            // Third step of the "state machine", add new fields to the union.
            let union_info = unions_found_temp
                .get_mut(&(curr_union_offset_range.start, 0))
                .expect("key should exist in map");
            union_info.0.end = i + 1;
            // Update the union's size
            union_info.1 = std::cmp::max(
                union_info.1,
                field.offset - curr_union_offset_range.start + field.size as u64,
            );
            curr_union_offset_range.end = std::cmp::max(
                curr_union_offset_range.end,
                field.offset + field.size as u64,
            );
            // (Re)visit previous fields to compute the union's size
            // as well as the current union's end
            for previous_field in &fields[union_info.0.clone()] {
                // Update the union's size
                if previous_field.offset > field.offset {
                    union_info.1 = std::cmp::max(
                        union_info.1,
                        previous_field.offset - field.offset + previous_field.size as u64,
                    );
                } else {
                    union_info.1 = std::cmp::max(union_info.1, previous_field.size as u64);
                }
                curr_union_offset_range.end = std::cmp::max(
                    curr_union_offset_range.end,
                    previous_field.offset + previous_field.size as u64,
                );
            }
        } else {
            match unions_found_temp
                .get_mut(&(field.offset, field.bitfield_info.unwrap_or_default().0))
            {
                Some(union_info) => {
                    // Second step of the "state machine", two fields share the
                    // same offset (taking bitfields into account). This becomes
                    // a union (the current one).
                    union_info.0.end = i + 1;
                    curr_union_offset_range.start = field.offset;
                    // (Re)visit previous fields to compute the union's size
                    // as well as the current union's end
                    for previous_field in &fields[union_info.0.clone()] {
                        // Update the union's size
                        if previous_field.offset > field.offset {
                            union_info.1 = std::cmp::max(
                                union_info.1,
                                previous_field.offset - field.offset + previous_field.size as u64,
                            );
                        } else {
                            union_info.1 = std::cmp::max(union_info.1, previous_field.size as u64);
                        }
                        curr_union_offset_range.end = std::cmp::max(
                            curr_union_offset_range.end,
                            previous_field.offset + previous_field.size as u64,
                        );
                    }
                }
                None => {
                    // First step of the "state machine".
                    // Each field is a potential new union
                    unions_found_temp.insert(
                        (field.offset, field.bitfield_info.unwrap_or_default().0),
                        (Range { start: i, end: i }, field.size as u64),
                    );
                }
            }
        }
    }

    // Remove nested unions, they will be processed when we'll go deeper
    for (offset1, range1) in unions_found_temp.iter() {
        let mut is_top_level = true;
        for (offset2, range2) in unions_found_temp.iter() {
            if offset1 == offset2 {
                // Comparing a union with itself, ignore
                continue;
            }
            if range1.0.start >= range2.0.start && range1.0.end < range2.0.end {
                // Union #1 is contained by Union #2, no need to continue.
                // Union #1 isn't a "top-level" union.
                is_top_level = false;
                break;
            }
        }
        if is_top_level {
            // Only keep "top-level" union
            unions_found.push(range1.0.clone());
        }
    }

    unions_found
}

fn fmt_union_fields_recursive(
    fmt_configuration: &DataFormatConfiguration,
    fields: &[Field],
    depth: usize,
    f: &mut impl std::fmt::Write,
) -> fmt::Result {
    if fields.is_empty() {
        return Ok(());
    }

    let structs_found = find_unnamed_structs_in_unions(fields);
    let indentation = "  ".repeat(depth);
    for struct_range in structs_found {
        // Fields out of unnamed structs are represented by "empty" structs
        if struct_range.is_empty() {
            let field = &fields[struct_range.start];
            writeln!(
                f,
                "{}/* {:#06x} */ {}{} {}{};{}",
                &indentation,
                field.offset,
                if fmt_configuration.print_access_specifiers {
                    &field.access
                } else {
                    &FieldAccess::None
                },
                field.type_left,
                field.name.to_string(),
                field.type_right,
                if let Some((bit_position, _)) = field.bitfield_info {
                    format!(" /* BitPos={bit_position} */")
                } else {
                    String::default()
                }
            )?;
        } else {
            writeln!(f, "{}struct {{", &indentation)?;
            fmt_struct_fields_recursive(fmt_configuration, &fields[struct_range], depth + 1, f)?;
            writeln!(f, "{}}};", &indentation)?;
        }
    }

    Ok(())
}

fn find_unnamed_structs_in_unions(fields: &[Field]) -> Vec<Range<usize>> {
    let mut structs_found: Vec<Range<usize>> = vec![];

    let field_count = fields.len();
    let union_offset = fields[0].offset;
    let mut previous_field_offset = fields[0].offset;
    let mut previous_field_bit_offset = fields[0].bitfield_info.unwrap_or_default().0;
    for (i, field) in fields.iter().enumerate() {
        // The field offset is lower than the offset of the previous field
        // -> "close" the struct
        if previous_field_offset > field.offset
            || (field.offset == previous_field_offset
                && previous_field_bit_offset > field.bitfield_info.unwrap_or_default().0)
        {
            if let Some(last_found_struct_range) = structs_found.pop() {
                // "Merge" previous field with the struct
                structs_found.push(Range {
                    start: last_found_struct_range.start,
                    end: i,
                });
            }
        }
        // Last element, check if we need to "close" a struct
        else if i == field_count - 1 {
            if let Some(last_found_struct_range) = structs_found.pop() {
                // Declare a new struct only if its length is greater than 1.
                if i > last_found_struct_range.start
                    && (field.offset != previous_field_offset
                        || field.bitfield_info.unwrap_or_default().0 != previous_field_bit_offset)
                {
                    // "Merge" previous field with the struct
                    structs_found.push(Range {
                        start: last_found_struct_range.start,
                        end: i + 1,
                    });
                } else {
                    structs_found.push(last_found_struct_range);
                }
            }
        }

        // Regular field, may be the beginning of a struct
        if field.offset == union_offset && field.bitfield_info.unwrap_or_default().0 == 0 {
            structs_found.push(Range { start: i, end: i });
        }

        previous_field_offset = field.offset;
        previous_field_bit_offset = field.bitfield_info.unwrap_or_default().0;
    }

    structs_found
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFormatConfiguration {
    pub print_access_specifiers: bool,
}

impl Default for DataFormatConfiguration {
    fn default() -> Self {
        Self {
            print_access_specifiers: true,
        }
    }
}
