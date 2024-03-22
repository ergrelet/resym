use std::path::Path;

use resym_core::{
    diffing::diff_type_by_name, pdb_file::PdbFile, pdb_types::PrimitiveReconstructionFlavor,
};

const TEST_PDB_FROM_FILE_PATH: &str = "tests/data/test_diff_from.pdb";
const TEST_PDB_TO_FILE_PATH: &str = "tests/data/test_diff_to.pdb";
const TEST_CASES: &[&str] = &[
    "UserStructAddAndReplace",
    "UserStructRemove",
    "UserStructAdd",
    "RemovedStruct",
    "NewStruct",
];

#[test]
fn test_struct_diffing() {
    let pdb_file_from = PdbFile::load_from_file(Path::new(TEST_PDB_FROM_FILE_PATH))
        .expect("load test_diff_from.pdb");
    let pdb_file_to =
        PdbFile::load_from_file(Path::new(TEST_PDB_TO_FILE_PATH)).expect("load test_diff_to.pdb");

    for test_case_type_name in TEST_CASES {
        let diffed_type = diff_type_by_name(
            &pdb_file_from,
            &pdb_file_to,
            test_case_type_name,
            PrimitiveReconstructionFlavor::Portable,
            false,
            false,
            false,
            false,
        )
        .expect("diff generation");
        insta::assert_snapshot!(diffed_type.data);
    }
}

#[test]
fn test_struct_diffing_inexistent_type() {
    const INEXISTENT_TYPE_NAME: &str = "TypeNotFound";
    let pdb_file_from = PdbFile::load_from_file(Path::new(TEST_PDB_FROM_FILE_PATH))
        .expect("load test_diff_from.pdb");
    let pdb_file_to =
        PdbFile::load_from_file(Path::new(TEST_PDB_TO_FILE_PATH)).expect("load test_diff_to.pdb");
    assert!(diff_type_by_name(
        &pdb_file_from,
        &pdb_file_to,
        INEXISTENT_TYPE_NAME,
        PrimitiveReconstructionFlavor::Portable,
        false,
        false,
        false,
        false,
    )
    .is_err());
}
