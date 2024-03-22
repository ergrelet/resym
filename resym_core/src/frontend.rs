use crate::{backend::PDBSlot, diffing::Diff, error::Result};

pub type TypeIndex = pdb::TypeIndex;
pub type TypeList = Vec<(String, TypeIndex)>;
pub type ModuleList = Vec<(String, usize)>;

pub enum FrontendCommand {
    LoadPDBResult(Result<PDBSlot>),
    /// Send result from `LoadURL` backend command.
    /// Contains last path segment (i.e., file name) as a `String` and data as `Vec<u8>`.
    LoadURLResult(Result<(PDBSlot, String, Vec<u8>)>),
    ListTypesResult(TypeList),
    ReconstructTypeResult(Result<String>),
    ReconstructModuleResult(Result<String>),
    UpdateModuleList(Result<ModuleList>),
    DiffResult(Result<Diff>),
    ListTypeCrossReferencesResult(Result<TypeList>),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
