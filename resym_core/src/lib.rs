pub mod backend;
pub mod diffing;
mod error;
pub mod frontend;
pub mod pdb_file;
pub mod pdb_types;
pub mod syntax_highlighting;

pub use error::*;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Macro used to switch between iterators depending on rayon's availability
#[macro_export]
#[cfg(not(feature = "rayon"))]
macro_rules! par_iter_if_available {
    ($expression:expr) => {
        $expression.iter()
    };
}
#[macro_export]
#[cfg(feature = "rayon")]
macro_rules! par_iter_if_available {
    ($expression:expr) => {
        $expression.par_iter()
    };
}

/// Macro used to switch between functions depending on rayon's availability
#[macro_export]
#[cfg(not(feature = "rayon"))]
macro_rules! par_sort_by_if_available {
    ($expression:expr, $($x:tt)*) => {
        $expression.sort_by($($x)*)
    };
}
#[macro_export]
#[cfg(feature = "rayon")]
macro_rules! par_sort_by_if_available {
    ($expression:expr, $($x:tt)*) => {
        $expression.par_sort_by($($x)*)
    };
}

/// Macro used to switch between `find_any` and `find` depending on rayon's availability
#[macro_export]
#[cfg(not(feature = "rayon"))]
macro_rules! find_any_if_available {
    ($expression:expr, $($x:tt)*) => {
        $expression.iter().find($($x)*)
    };
}
#[macro_export]
#[cfg(feature = "rayon")]
macro_rules! find_any_if_available {
    ($expression:expr, $($x:tt)*) => {
        $expression.par_iter().find_any($($x)*)
    };
}
