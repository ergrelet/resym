use std::fmt;

use super::{DataFormatConfiguration, ReconstructibleTypeData};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardReference {
    pub index: pdb::TypeIndex,
    pub kind: pdb::ClassKind,
    pub name: String,
}

impl ReconstructibleTypeData for ForwardReference {
    fn reconstruct(
        &self,
        _fmt_configuration: &DataFormatConfiguration,
        f: &mut impl std::fmt::Write,
    ) -> fmt::Result {
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
