pub mod backend;
pub mod diffing;
pub mod frontend;
pub mod pdb_file;
pub mod pdb_types;
pub mod syntax_highlighting;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
