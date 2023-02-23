#[cfg(target_arch = "wasm32")]
use instant::Instant;
use similar::{ChangeTag, TextDiff};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{fmt::Write, io};

use crate::{
    error::{Result, ResymCoreError},
    pdb_file::PdbFile,
    pdb_types::PrimitiveReconstructionFlavor,
    PKG_VERSION,
};

pub type DiffChange = ChangeTag;
pub type DiffIndices = (Option<usize>, Option<usize>);

#[derive(Default)]
pub struct DiffedType {
    pub metadata: Vec<(DiffIndices, DiffChange)>,
    pub data: String,
}
pub struct DiffLine {
    pub indices: DiffIndices,
    pub change: DiffChange,
    pub line: String,
}

pub fn diff_type_by_name<'p, T>(
    pdb_file_from: &PdbFile<'p, T>,
    pdb_file_to: &PdbFile<'p, T>,
    type_name: &str,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> Result<DiffedType>
where
    T: io::Seek + io::Read + 'p,
{
    let diff_start = Instant::now();
    // Prepend header if needed
    let (mut reconstructed_type_from, mut reconstructed_type_to) = if print_header {
        let diff_header = generate_diff_header(pdb_file_from, pdb_file_to);
        (diff_header.clone(), diff_header)
    } else {
        (String::default(), String::default())
    };

    // Reconstruct type from both PDBs
    {
        let reconstructed_type_from_tmp = pdb_file_from
            .reconstruct_type_by_name(
                type_name,
                primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
            )
            .unwrap_or_default();
        let reconstructed_type_to_tmp = pdb_file_to
            .reconstruct_type_by_name(
                type_name,
                primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
            )
            .unwrap_or_default();
        if reconstructed_type_from_tmp.is_empty() && reconstructed_type_to_tmp.is_empty() {
            // Make it obvious an error occured
            return Err(ResymCoreError::TypeNameNotFoundError(type_name.to_owned()));
        }
        reconstructed_type_from.push_str(&reconstructed_type_from_tmp);
        reconstructed_type_to.push_str(&reconstructed_type_to_tmp);
    }

    // Diff reconstructed reprensentations
    let mut diff_metadata = vec![];
    let mut diff_data = String::default();
    {
        let reconstructed_type_diff =
            TextDiff::from_lines(&reconstructed_type_from, &reconstructed_type_to);
        for change in reconstructed_type_diff.iter_all_changes() {
            diff_metadata.push(((change.old_index(), change.new_index()), change.tag()));
            let prefix = match change.tag() {
                ChangeTag::Insert => "+",
                ChangeTag::Delete => "-",
                ChangeTag::Equal => " ",
            };
            write!(&mut diff_data, "{prefix}{change}")?;
        }
    }

    log::debug!("Type diffing took {} ms", diff_start.elapsed().as_millis());

    Ok(DiffedType {
        metadata: diff_metadata,
        data: diff_data,
    })
}

fn generate_diff_header<'p, T>(
    pdb_file_from: &PdbFile<'p, T>,
    pdb_file_to: &PdbFile<'p, T>,
) -> String
where
    T: io::Seek + io::Read + 'p,
{
    format!(
        concat!(
            "//\n",
            "// Showing differences between two PDB files:\n",
            "//\n",
            "// Reference PDB file: {}\n",
            "// Image architecture: {}\n",
            "//\n",
            "// New PDB file: {}\n",
            "// Image architecture: {}\n",
            "//\n",
            "// Information extracted with resym v{}\n",
            "//\n"
        ),
        pdb_file_from.file_path.display(),
        pdb_file_from.machine_type,
        pdb_file_to.file_path.display(),
        pdb_file_to.machine_type,
        PKG_VERSION,
    )
}
