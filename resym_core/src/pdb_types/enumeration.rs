use std::fmt;

use super::{DataFormatConfiguration, NeededTypeSet, ReconstructibleTypeData};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum<'p> {
    pub index: pdb::TypeIndex,
    pub name: String,
    pub underlying_type_name: String,
    pub values: Vec<EnumValue<'p>>,
}

impl<'p> Enum<'p> {
    pub fn add_fields(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_index: pdb::TypeIndex,
        needed_types: &mut NeededTypeSet,
    ) -> Result<()> {
        match type_finder.find(type_index)?.parse()? {
            pdb::TypeData::FieldList(data) => {
                for field in &data.fields {
                    self.add_field(type_finder, field, needed_types);
                }

                if let Some(continuation) = data.continuation {
                    // recurse
                    self.add_fields(type_finder, continuation, needed_types)?;
                }
            }
            other => {
                log::warn!(
                    "trying to Enum::add_fields() got {} -> {:?}",
                    type_index,
                    other
                );
            }
        }

        Ok(())
    }

    fn add_field(
        &mut self,
        _: &pdb::TypeFinder<'p>,
        field: &pdb::TypeData<'p>,
        _: &mut NeededTypeSet,
    ) {
        // ignore everything else even though that's sad
        if let pdb::TypeData::Enumerate(ref data) = field {
            self.values.push(EnumValue {
                name: data.name,
                value: data.value,
            });
        }
    }
}

impl ReconstructibleTypeData for Enum<'_> {
    fn reconstruct(
        &self,
        fmt_configuration: &DataFormatConfiguration,
        f: &mut impl std::fmt::Write,
    ) -> fmt::Result {
        writeln!(f, "enum {} : {} {{", self.name, self.underlying_type_name)?;

        for value in &self.values {
            writeln!(
                f,
                "  {} = {},",
                value.name.to_string(),
                match value.value {
                    pdb::Variant::U8(v) => {
                        if fmt_configuration.integers_as_hexadecimal {
                            format!("0x{v:02x}")
                        } else {
                            format!("{v}")
                        }
                    }
                    pdb::Variant::U16(v) => {
                        if fmt_configuration.integers_as_hexadecimal {
                            format!("0x{v:04x}")
                        } else {
                            format!("{v}")
                        }
                    }
                    pdb::Variant::U32(v) => {
                        if fmt_configuration.integers_as_hexadecimal {
                            format!("0x{v:08x}")
                        } else {
                            format!("{v}")
                        }
                    }
                    pdb::Variant::U64(v) => {
                        if fmt_configuration.integers_as_hexadecimal {
                            format!("0x{v:16x}")
                        } else {
                            format!("{v}")
                        }
                    }
                    pdb::Variant::I8(v) => format!("{v}"),
                    pdb::Variant::I16(v) => format!("{v}"),
                    pdb::Variant::I32(v) => format!("{v}"),
                    pdb::Variant::I64(v) => format!("{v}"),
                }
            )?;
        }
        writeln!(f, "}};")?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue<'p> {
    name: pdb::RawString<'p>,
    value: pdb::Variant,
}
