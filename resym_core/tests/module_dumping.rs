use std::path::Path;

use resym_core::{pdb_file::PdbFile, pdb_types::PrimitiveReconstructionFlavor};

const TEST_PDB_FILE_PATH: &str = "tests/data/test.pdb";
const TEST_MODULE_INDEX: usize = 27;
const TEST_MODULE_PATH: &str = "D:\\a\\_work\\1\\s\\Intermediate\\crt\\vcstartup\\build\\xmd\\msvcrt_kernel32\\msvcrt_kernel32.nativeproj\\objd\\amd64\\default_local_stdio_options.obj";

#[test]
fn test_module_dumping_by_path_portable() {
    let pdb_file = PdbFile::load_from_file(Path::new(TEST_PDB_FILE_PATH)).expect("load test.pdb");

    let module_dump = pdb_file
        .reconstruct_module_by_path(TEST_MODULE_PATH, PrimitiveReconstructionFlavor::Portable)
        .unwrap_or_else(|err| panic!("module dumping failed: {err}"));

    insta::assert_snapshot!("module_dumping_by_path_portable", module_dump);
}

#[test]
fn test_module_dumping_by_index_portable() {
    test_module_dumping_by_index_internal(
        "module_dumping_by_index_portable",
        TEST_MODULE_INDEX,
        PrimitiveReconstructionFlavor::Portable,
    );
}

#[test]
fn test_module_dumping_by_index_microsoft() {
    test_module_dumping_by_index_internal(
        "module_dumping_by_index_microsoft",
        TEST_MODULE_INDEX,
        PrimitiveReconstructionFlavor::Microsoft,
    );
}

#[test]
fn test_module_dumping_by_index_raw() {
    test_module_dumping_by_index_internal(
        "module_dumping_by_index_raw",
        TEST_MODULE_INDEX,
        PrimitiveReconstructionFlavor::Raw,
    );
}

fn test_module_dumping_by_index_internal(
    snapshot_name: &str,
    module_index: usize,
    primitives_flavor: PrimitiveReconstructionFlavor,
) {
    let pdb_file = PdbFile::load_from_file(Path::new(TEST_PDB_FILE_PATH)).expect("load test.pdb");

    let module_dump = pdb_file
        .reconstruct_module_by_index(module_index, primitives_flavor)
        .unwrap_or_else(|_| panic!("module dumping"));

    insta::assert_snapshot!(snapshot_name, module_dump);
}
