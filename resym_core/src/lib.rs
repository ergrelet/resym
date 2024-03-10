pub mod backend;
pub mod diffing;
mod error;
pub mod frontend;
pub mod pdb_file;
pub mod pdb_types;
pub mod rayon_utils;
pub mod syntax_highlighting;

pub use error::*;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
