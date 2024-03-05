use std::path::Path;

use resym_core::pdb_file::PdbFile;

const TEST_PDB_FILE_PATH: &str = "tests/data/test.pdb";

#[test]
fn test_module_listing() {
    let pdb_file = PdbFile::load_from_file(Path::new(TEST_PDB_FILE_PATH)).expect("load test.pdb");

    let module_list = pdb_file
        .module_list()
        .unwrap_or_else(|err| panic!("module listing failed: {err}"));

    let snapshot_name = "module_listing";
    let snapshot_data = module_list
        .into_iter()
        .fold(String::new(), |acc, (mod_name, mod_id)| {
            format!("{acc}\n{mod_id} {mod_name}")
        });
    insta::assert_snapshot!(snapshot_name, snapshot_data);
}
