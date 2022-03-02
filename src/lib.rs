pub mod backend;
pub mod pdb_file;
pub mod pdb_types;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");
pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum UICommand {
    UpdateFilteredSymbols(Vec<(String, pdb::TypeIndex)>),
    UpdateReconstructedType(String),
}
