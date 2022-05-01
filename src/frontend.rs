use anyhow::Result;

pub enum FrontendCommand {
    UpdateFilteredSymbols(Vec<(String, pdb::TypeIndex)>),
    UpdateReconstructedType(String),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
