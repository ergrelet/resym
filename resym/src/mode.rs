use resym_core::diffing::DiffChange;

#[derive(PartialEq)]
pub enum ResymAppMode {
    /// Mode in which the application starts
    Idle,
    /// This mode means we're browsing a single PDB file
    Browsing(String, usize, String),
    /// This mode means we're comparing two PDB files for differences
    Comparing(String, String, usize, Vec<DiffChange>, String),
}
