use std::fmt;

use anyhow::Result;

use super::TypeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum<'p> {
    pub name: String,
    pub underlying_type_name: String,
    pub values: Vec<EnumValue<'p>>,
}

impl<'p> Enum<'p> {
    pub fn add_fields(
        &mut self,
        type_finder: &pdb::TypeFinder<'p>,
        type_index: pdb::TypeIndex,
        needed_types: &mut TypeSet,
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

    fn add_field(&mut self, _: &pdb::TypeFinder<'p>, field: &pdb::TypeData<'p>, _: &mut TypeSet) {
        // ignore everything else even though that's sad
        if let pdb::TypeData::Enumerate(ref data) = field {
            self.values.push(EnumValue {
                name: data.name,
                value: data.value,
            });
        }
    }

    pub fn reconstruct(&self, f: &mut impl std::fmt::Write) -> fmt::Result {
        writeln!(f, "enum {} : {} {{", self.name, self.underlying_type_name)?;

        for value in &self.values {
            writeln!(
                f,
                "  {} = {},",
                value.name.to_string(),
                match value.value {
                    pdb::Variant::U8(v) => format!("0x{:02x}", v),
                    pdb::Variant::U16(v) => format!("0x{:04x}", v),
                    pdb::Variant::U32(v) => format!("0x{:08x}", v),
                    pdb::Variant::U64(v) => format!("0x{:16x}", v),
                    pdb::Variant::I8(v) => format!("{}", v),
                    pdb::Variant::I16(v) => format!("{}", v),
                    pdb::Variant::I32(v) => format!("{}", v),
                    pdb::Variant::I64(v) => format!("{}", v),
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
