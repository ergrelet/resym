use crate::{backend::PDBSlot, diffing::DiffedType, error::Result};

pub type TypeList = Vec<(String, pdb::TypeIndex)>;

pub enum FrontendCommand {
    LoadPDBResult(Result<PDBSlot>),
    /// Send result from `LoadURL` backend command.
    /// Contains last path segment (i.e., file name) as a `String` and data as `Vec<u8>`.
    LoadURLResult(Result<(PDBSlot, String, Vec<u8>)>),
    UpdateFilteredTypes(TypeList),
    ReconstructTypeResult(Result<String>),
    DiffTypeResult(Result<DiffedType>),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
