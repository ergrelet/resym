use anyhow::Result;

pub enum FrontendCommand {
    UpdateFilteredTypes(Vec<(String, pdb::TypeIndex)>),
    UpdateReconstructedType(String),
}

pub trait FrontendController {
    fn send_command(&self, command: FrontendCommand) -> Result<()>;
}
