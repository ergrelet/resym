use resym_core::pdb_types::PrimitiveReconstructionFlavor;
use serde::{Deserialize, Serialize};

/// This struct represents the persistent settings of the application.
#[derive(Serialize, Deserialize)]
pub struct ResymAppSettings {
    pub use_light_theme: bool,
    pub font_size: u16,
    pub search_case_insensitive: bool,
    pub search_use_regex: bool,
    pub enable_syntax_hightlighting: bool,
    pub integers_as_hexadecimal: bool,
    #[serde(with = "PrimitiveReconstructionFlavorDef")]
    pub primitive_types_flavor: PrimitiveReconstructionFlavor,
    pub print_header: bool,
    pub reconstruct_dependencies: bool,
    pub print_access_specifiers: bool,
    // Ignore types in the `std` namespace (e.g., STL-generated types)
    pub ignore_std_types: bool,
    pub print_line_numbers: bool,
}

impl Default for ResymAppSettings {
    fn default() -> Self {
        Self {
            use_light_theme: false,
            font_size: 14,
            search_case_insensitive: true,
            search_use_regex: false,
            enable_syntax_hightlighting: true,
            integers_as_hexadecimal: true,
            primitive_types_flavor: PrimitiveReconstructionFlavor::Portable,
            print_header: true,
            reconstruct_dependencies: true,
            print_access_specifiers: true,
            ignore_std_types: true,
            print_line_numbers: false,
        }
    }
}

// Definition of the remote enum so that serde can its traits
#[derive(Serialize, Deserialize)]
#[serde(remote = "PrimitiveReconstructionFlavor")]
enum PrimitiveReconstructionFlavorDef {
    Portable,
    Microsoft,
    Raw,
    Msvc,
}
