mod class;
mod enumeration;
mod field;
mod method;
mod primitive_types;
mod union;

use std::collections::{BTreeMap, BTreeSet};
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

/// Set of `TypeIndex` objets
pub type TypeSet = BTreeSet<pdb::TypeIndex>;

pub type TypeForwarder = dashmap::DashMap<pdb::TypeIndex, pdb::TypeIndex>;

/// Return a pair of strings representing the given `type_index`.
pub fn type_name<'p>(
    type_finder: &pdb::TypeFinder<'p>,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut TypeSet,
) -> Result<(String, String)> {
    let (type_left, type_right) = match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Primitive(data) => {
            let name =
                primitive_kind_as_str(primitive_flavor, data.kind, data.indirection.is_some())?;

            (name, String::default())
        }

        pdb::TypeData::Class(data) => {
            needed_types.insert(type_index);
            // Rename unnamed anonymous tags to something unique
            let name = data.name.to_string();
            if is_unnamed_type(&name) {
                let name = format!("_unnamed_{}", type_index);
                (name, String::default())
            } else {
                (name.into_owned(), String::default())
            }
        }

        pdb::TypeData::Union(data) => {
            needed_types.insert(type_index);
            // Rename unnamed anonymous tags to something unique
            let name = data.name.to_string();
            if is_unnamed_type(&name) {
                let name = format!("_unnamed_{}", type_index);
                (name, String::default())
            } else {
                (name.into_owned(), String::default())
            }
        }

        pdb::TypeData::Enumeration(data) => {
            needed_types.insert(type_index);
            (data.name.to_string().into_owned(), String::default())
        }

        pdb::TypeData::Pointer(data) => {
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
            if data.attributes.is_reference() {
                (format!("{}&", type_left), type_right)
            } else {
                (format!("{}*", type_left), type_right)
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
                (format!("const {}", type_left), type_right)
            } else if data.volatile {
                (format!("volatile {}", type_left), type_right)
            } else {
                // ?
                (type_left, type_right)
            }
        }

        pdb::TypeData::Array(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_element_type_index =
                resolve_complete_type_index(type_forwarder, data.element_type);
            let (base_name, mut dimensions) = array_base_name(
                type_finder,
                type_forwarder,
                complete_element_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let type_size = u32::try_from(type_size(type_finder, complete_element_type_index)?)?;
            let mut divider = if type_size == 0 {
                log::warn!(
                    "'{}' has invalid size (0), array dimensions might be incorrect",
                    base_name
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
                dimensions_str = format!("{}[{}]", dimensions_str, dim);
            }

            (base_name, dimensions_str)
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
                format!("{}{} (", ret_type_left, ret_type_right),
                format!(")({})", arg_list.join(", ")),
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
                format!("{}{} ({}::", ret_type_left, ret_type_right, class_type_left),
                format!(")({})", arg_list.join(", ")),
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

fn array_base_name<'p>(
    type_finder: &pdb::TypeFinder<'p>,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut TypeSet,
) -> Result<(String, Vec<usize>)> {
    match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::Array(data) => {
            // Resolve the complete type's index, if present in the PDB
            let complete_element_type_index =
                resolve_complete_type_index(type_forwarder, data.element_type);
            let (base_name, mut base_dimensions) = array_base_name(
                type_finder,
                type_forwarder,
                complete_element_type_index,
                primitive_flavor,
                needed_types,
            )?;
            let type_size = u32::try_from(type_size(type_finder, complete_element_type_index)?)?;
            let mut divider = if type_size == 0 {
                log::warn!(
                    "'{}' has invalid size (0), array dimensions might be incorrect",
                    base_name
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

            Ok((base_name, base_dimensions))
        }
        _ => Ok((
            type_name(
                type_finder,
                type_forwarder,
                type_index,
                primitive_flavor,
                needed_types,
            )?
            .0,
            vec![],
        )),
    }
}

pub fn argument_list<'p>(
    type_finder: &pdb::TypeFinder<'p>,
    type_forwarder: &TypeForwarder,
    type_index: pdb::TypeIndex,
    primitive_flavor: &PrimitiveReconstructionFlavor,
    needed_types: &mut TypeSet,
) -> Result<Vec<String>> {
    match type_finder.find(type_index)?.parse()? {
        pdb::TypeData::ArgumentList(data) => {
            let mut args: Vec<String> = Vec::new();
            for arg_type in data.arguments {
                args.push(
                    type_name(
                        type_finder,
                        type_forwarder,
                        arg_type,
                        primitive_flavor,
                        needed_types,
                    )?
                    .0,
                );
            }
            Ok(args)
        }
        _ => Err(ResymCoreError::InvalidParameterError(
            "argument list of non-argument-list type".to_owned(),
        )),
    }
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

/// Struct that represent a set of reconstructed types (forward declarations,
/// classes/structs, enums and unions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Data<'p> {
    forward_references: Vec<ForwardReference>,
    classes: Vec<Class<'p>>,
    enums: Vec<Enum<'p>>,
    unions: Vec<Union<'p>>,
}

impl Data<'_> {
    pub fn reconstruct(
        &self,
        fmt_configuration: &DataFormatConfiguration,
        f: &mut impl std::fmt::Write,
    ) -> fmt::Result {
        // Types without definition
        if !self.forward_references.is_empty() {
            writeln!(f)?;
            for e in &self.forward_references {
                e.reconstruct(f)?;
            }
        }

        // Enum definitions
        for e in &self.enums {
            writeln!(f)?;
            e.reconstruct(f)?;
        }

        // Class/struct definitions
        for class in &self.classes {
            writeln!(f)?;
            class.reconstruct(fmt_configuration, f)?;
        }

        // Union definitions
        for u in &self.unions {
            writeln!(f)?;
            u.reconstruct(fmt_configuration, f)?;
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
            forward_references: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            unions: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_forwarder: &TypeForwarder,
        type_index: pdb::TypeIndex,
        primitive_flavor: &PrimitiveReconstructionFlavor,
        needed_types: &mut TypeSet,
    ) -> Result<()> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::Class(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{}", type_index)
                } else {
                    name_str.into_owned()
                };

                if data.properties.forward_reference() {
                    self.forward_references.push(ForwardReference {
                        kind: data.kind,
                        name,
                    });

                    return Ok(());
                }

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

                self.classes.insert(0, class);
            }

            pdb::TypeData::Union(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{}", type_index)
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

                self.unions.insert(0, u);
            }

            pdb::TypeData::Enumeration(data) => {
                let name_str = data.name.to_string();
                // Rename unnamed anonymous tags to something unique
                let name = if is_unnamed_type(&name_str) {
                    format!("_unnamed_{}", type_index)
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

                if let Err(err) = e.add_fields(type_finder, data.fields, needed_types) {
                    log::error!(
                        "Error encountered while reconstructing '{}': {}",
                        e.name,
                        err
                    );
                }

                self.enums.insert(0, e);
            }

            // ignore
            other => log::error!("warning: don't know how to add {:?}", other),
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
    for union_range in unions_found {
        // Fields out of unnamed unions are represented by "empty" unions
        if union_range.is_empty() {
            let field = &fields[union_range.start];
            writeln!(
                f,
                "{}/* {:#06x} */ {}{} {}{};",
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
            )?;
        } else {
            writeln!(f, "{}union {{", &indentation)?;
            fmt_union_fields_recursive(fmt_configuration, &fields[union_range], depth + 1, f)?;
            writeln!(f, "{}}};", &indentation)?;
        }
    }

    Ok(())
}

fn find_unnamed_unions_in_struct(fields: &[Field]) -> Vec<Range<usize>> {
    let mut unions_found: Vec<Range<usize>> = vec![];
    // Temporary map of unions and fields that'll be used to compute the list
    // of unnamed unions which are in the struct.
    let mut unions_found_temp: BTreeMap<u64, (Range<usize>, u64)> = BTreeMap::new();

    // Discover unions
    let mut curr_union_offset_range: Range<u64> = Range::default();
    for (i, field) in fields.iter().enumerate() {
        // Check if the field is located inside of the union we're processing
        if curr_union_offset_range.contains(&field.offset) {
            let union_info = unions_found_temp
                .get_mut(&curr_union_offset_range.start)
                .unwrap();
            union_info.0.end = i + 1;
            // Update the union's size
            union_info.1 = std::cmp::max(union_info.1, field.size as u64);
            curr_union_offset_range.end = std::cmp::max(
                curr_union_offset_range.end,
                field.offset + field.size as u64,
            );
        } else {
            match unions_found_temp.get_mut(&field.offset) {
                Some(union_info) => {
                    union_info.0.end = i + 1;
                    curr_union_offset_range.start = field.offset;
                    // (Re)visit previous fields to compute the union's size
                    // as well as the current union's end
                    for previous_field in &fields[union_info.0.clone()] {
                        // Update the union's size
                        union_info.1 = std::cmp::max(union_info.1, previous_field.size as u64);
                        curr_union_offset_range.end = std::cmp::max(
                            curr_union_offset_range.end,
                            previous_field.offset + previous_field.size as u64,
                        );
                    }
                }
                None => {
                    unions_found_temp.insert(
                        field.offset,
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
                "{}/* {:#06x} */ {}{} {}{};",
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
    for (i, field) in fields.iter().enumerate() {
        // The field offset is lower than the offset of the previous field,
        // "close" the struct
        if previous_field_offset > field.offset {
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
                if i > last_found_struct_range.start && field.offset != previous_field_offset {
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
        if field.offset == union_offset {
            structs_found.push(Range { start: i, end: i });
        }

        previous_field_offset = field.offset;
    }

    structs_found
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForwardReference {
    kind: pdb::ClassKind,
    name: String,
}

impl ForwardReference {
    pub fn reconstruct(&self, f: &mut impl std::fmt::Write) -> fmt::Result {
        writeln!(
            f,
            "{} {};",
            match self.kind {
                pdb::ClassKind::Class => "class",
                pdb::ClassKind::Struct => "struct",
                pdb::ClassKind::Interface => "interface", // when can this happen?
            },
            self.name
        )
    }
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
