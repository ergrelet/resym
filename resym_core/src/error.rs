use thiserror::Error;

pub type Result<T> = std::result::Result<T, ResymCoreError>;

/// Error type used across `resym_core`
#[derive(Error, Debug)]
pub enum ResymCoreError {
    /// Error reported from `std::io`.
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    /// Error reported from `std::fmt`.
    #[error(transparent)]
    FmtError(#[from] std::fmt::Error),

    /// Error reported from `pdb`.
    #[error("pdb error: {0}")]
    PdbError(#[from] pdb::Error),

    /// Error reported from `rayon`.
    #[cfg(feature = "rayon")]
    #[error("rayon error: {0}")]
    RayonError(#[from] rayon::ThreadPoolBuildError),

    /// Error reported from `crossbeam_channel`.
    #[error("crossbeam error: {0}")]
    CrossbeamError(String),

    /// Error reported in case of int conversion failures.
    #[error("int conversion error: {0}")]
    TryFromIntError(#[from] std::num::TryFromIntError),

    /// Error reported from `ehttp`.
    #[cfg(feature = "http")]
    #[error("http error: {0}")]
    EHttpError(String),

    /// Error returned when `resym_core` cannot process the request because of
    /// of an invalid parameter.
    #[error("invalid parameter: {0}")]
    InvalidParameterError(String),

    /// Error returned when querying for a type by name, that isn't present in
    /// the PDB file.
    #[error("type not found: {0}")]
    TypeNameNotFoundError(String),

    /// Error returned when querying for a module's information, that isn't available in
    /// the PDB file.
    #[error("module info not found: {0}")]
    ModuleInfoNotFoundError(String),

    /// Error returned when parsing a `PrimitiveReconstructionFlavor` from a string fails.
    #[error("invalid primitive type flavor: {0}")]
    ParsePrimitiveFlavorError(String),

    /// Error returned when `resym_core` cannot process the request because of
    /// unimplemented features.
    #[error("feature not implemented: {0}")]
    NotImplementedError(String),
}
