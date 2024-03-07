use std::path::Path;

use resym_core::{
    diffing::diff_module_by_path, pdb_file::PdbFile, pdb_types::PrimitiveReconstructionFlavor,
};

const TEST_PDB_FROM_FILE_PATH: &str = "tests/data/test_diff_from.pdb";
const TEST_PDB_TO_FILE_PATH: &str = "tests/data/test_diff_to.pdb";
// TODO(ergrelet): replace with a more interesting module when support for more
// symbol kinds is implemented in the `pdb` crate
const TEST_MODULE_PATH: &str = "d:\\a01\\_work\\43\\s\\Intermediate\\vctools\\msvcrt.nativeproj_607447030\\objd\\amd64\\exe_main.obj";

#[test]
fn test_module_diffing_by_path() {
    let mut pdb_file_from = PdbFile::load_from_file(Path::new(TEST_PDB_FROM_FILE_PATH))
        .expect("load test_diff_from.pdb");
    let mut pdb_file_to =
        PdbFile::load_from_file(Path::new(TEST_PDB_TO_FILE_PATH)).expect("load test_diff_to.pdb");

    let module_diff = diff_module_by_path(
        &mut pdb_file_from,
        &mut pdb_file_to,
        TEST_MODULE_PATH,
        PrimitiveReconstructionFlavor::Portable,
        true,
    )
    .unwrap_or_else(|err| panic!("module diffing failed: {err}"));

    insta::assert_snapshot!("module_diffing_by_path", module_diff.data);
}
