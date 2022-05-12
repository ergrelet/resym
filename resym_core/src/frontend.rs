use anyhow::Result;

use crate::backend::PDBSlot;

pub enum FrontendCommand {
    LoadPDBResult(Result<PDBSlot>),
    UpdateFilteredTypes(Vec<(String, pdb::TypeIndex)>),
    UpdateReconstructedType(String),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
