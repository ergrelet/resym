use std::path::PathBuf;

use resym_core::pdb_types::PrimitiveReconstructionFlavor;
use structopt::StructOpt;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, StructOpt)]
#[structopt(
    name = PKG_NAME,
    about = "resymc is a utility that allows browsing and extracting types from PDB files."
)]
pub enum ResymcOptions {
    /// List types from a given PDB file
    List {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Search filter
        type_name_filter: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Do not match case
        #[structopt(short = "i", long)]
        case_insensitive: bool,
        /// Use regular expressions
        #[structopt(short = "r", long)]
        use_regex: bool,
        /// Filter out types in the `std` namespace
        #[structopt(short = "s", long)]
        ignore_std_types: bool,
    },
    /// Dump type from a given PDB file
    Dump {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Name of the type to extract
        type_name: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Representation of primitive types
        #[structopt(short = "f", long)]
        primitive_types_flavor: Option<PrimitiveReconstructionFlavor>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Print declarations of referenced types
        #[structopt(short = "d", long)]
        print_dependencies: bool,
        /// Print C++ access specifiers
        #[structopt(short = "a", long)]
        print_access_specifiers: bool,
        /// Filter out types in the `std` namespace
        #[structopt(short = "s", long)]
        ignore_std_types: bool,
        /// Highlight C++ output
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
    /// Dump all types from a given PDB file
    DumpAll {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Representation of primitive types
        #[structopt(short = "f", long)]
        primitive_types_flavor: Option<PrimitiveReconstructionFlavor>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Print C++ access specifiers
        #[structopt(short = "a", long)]
        print_access_specifiers: bool,
        /// Filter out types in the `std` namespace
        #[structopt(short = "s", long)]
        ignore_std_types: bool,
        /// Highlight C++ output
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
    /// Compute diff for a type between two given PDB files
    Diff {
        /// Path of the PDB file to compute the diff from
        from_pdb_path: PathBuf,
        /// Path of the PDB file to compute the diff to
        to_pdb_path: PathBuf,
        /// Name of the type to diff
        type_name: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Representation of primitive types
        #[structopt(short = "f", long)]
        primitive_types_flavor: Option<PrimitiveReconstructionFlavor>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Print declarations of referenced types
        #[structopt(short = "d", long)]
        print_dependencies: bool,
        /// Print C++ access specifiers
        #[structopt(short = "a", long)]
        print_access_specifiers: bool,
        /// Filter out types in the `std` namespace
        #[structopt(short = "s", long)]
        ignore_std_types: bool,
        /// Highlight C++ output and add/deleted lines
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
    /// List modules from a given PDB file
    ListModules {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Search filter
        module_path_filter: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Do not match case
        #[structopt(short = "i", long)]
        case_insensitive: bool,
        /// Use regular expressions
        #[structopt(short = "r", long)]
        use_regex: bool,
    },
    /// Dump module from a given PDB file
    DumpModule {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// ID of the module to dump
        module_id: usize,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Representation of primitive types
        #[structopt(short = "f", long)]
        primitive_types_flavor: Option<PrimitiveReconstructionFlavor>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Highlight C++ output
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
    /// Compute diff for a module between two given PDB files
    DiffModule {
        /// Path of the PDB file to compute the diff from
        from_pdb_path: PathBuf,
        /// Path of the PDB file to compute the diff to
        to_pdb_path: PathBuf,
        /// Path of the module to diff
        module_path: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Representation of primitive types
        #[structopt(short = "f", long)]
        primitive_types_flavor: Option<PrimitiveReconstructionFlavor>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Highlight C++ output and add/deleted lines
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
}
