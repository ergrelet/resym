use anyhow::Result;

use crate::backend::PDBSlot;

pub type TypeList = Vec<(String, pdb::TypeIndex)>;

pub enum FrontendCommand {
    LoadPDBResult(Result<PDBSlot>),
    UpdateFilteredTypes(TypeList),
    UpdateReconstructedType(String),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
