use std::path::Path;

use resym::{diffing::diff_type_by_name, pdb_file::PdbFile};

const TEST_PDB_FROM_FILE_PATH: &str = "tests/data/test_diff_from.pdb";
const TEST_PDB_TO_FILE_PATH: &str = "tests/data/test_diff_to.pdb";
const TEST_CASES: &[&str] = &[
    "UserStructAddAndReplace",
    "UserStructRemove",
    "UserStructAdd",
    "RemovedStruct",
    "NewStruct",
    "TypeNotFound",
];

#[test]
fn test_struct_diffing_no_dependencies_without_line_numbers() {
    let pdb_file_from = PdbFile::load_from_file(Path::new(TEST_PDB_FROM_FILE_PATH))
        .expect("load test_diff_from.pdb");
    let pdb_file_to =
        PdbFile::load_from_file(Path::new(TEST_PDB_TO_FILE_PATH)).expect("load test_diff_to.pdb");

    for test_case_type_name in TEST_CASES {
        let diffed_type = diff_type_by_name(
            &pdb_file_from,
            &pdb_file_to,
            test_case_type_name,
            false,
            false,
            false,
            false,
        );
        insta::assert_snapshot!(diffed_type);
    }
}

#[test]
fn test_struct_diffing_no_dependencies_with_line_numbers() {
    let pdb_file_from = PdbFile::load_from_file(Path::new(TEST_PDB_FROM_FILE_PATH))
        .expect("load test_diff_from.pdb");
    let pdb_file_to =
        PdbFile::load_from_file(Path::new(TEST_PDB_TO_FILE_PATH)).expect("load test_diff_to.pdb");

    for test_case_type_name in TEST_CASES {
        let diffed_type = diff_type_by_name(
            &pdb_file_from,
            &pdb_file_to,
            test_case_type_name,
            false,
            false,
            false,
            true,
        );
        insta::assert_snapshot!(diffed_type);
    }
}
