use crate::{
    backend::PDBSlot,
    diffing::Diff,
    error::Result,
    pdb_file::{ModuleList, SymbolList, TypeList},
};

/// Tuple containing the reconstructed type as a `String`
/// and the list of directly referenced types as a `TypeList`
pub type ReconstructedType = (String, TypeList);

pub enum FrontendCommand {
    LoadPDBResult(Result<PDBSlot>),
    /// Send result from `LoadURL` backend command.
    /// Contains last path segment (i.e., file name) as a `String` and data as `Vec<u8>`.
    LoadURLResult(Result<(PDBSlot, String, Vec<u8>)>),

    // Types
    ListTypesResult(TypeList),
    ReconstructTypeResult(Result<ReconstructedType>),

    // Symbols
    ListSymbolsResult(SymbolList),
    ReconstructSymbolResult(Result<String>),

    // Modules
    ListModulesResult(Result<ModuleList>),
    ReconstructModuleResult(Result<String>),

    // Diff
    DiffResult(Result<Diff>),
    // Xrefs
    ListTypeCrossReferencesResult(Result<TypeList>),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
