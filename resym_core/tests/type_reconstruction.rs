use std::path::Path;

use resym_core::{pdb_file::PdbFile, pdb_types::PrimitiveReconstructionFlavor};

const TEST_PDB_FILE_PATH: &str = "tests/data/test.pdb";
const TEST_CASES: &[&str] = &[
    "resym_test::PrimitiveTypesTest",
    "resym_test::ArrayTest",
    "resym_test::BitFieldsTest1",
    "resym_test::BitFieldsTest2",
    "resym_test::UnionTest",
    "resym_test::StructTest",
    "resym_test::EnumTest1",
    "resym_test::EnumTest2",
    "resym_test::StructUnnamedUdtTest1",
    "resym_test::StructUnnamedUdtTest2",
    "resym_test::StructUnnamedUdtTest3",
    "resym_test::UnionUnnamedUdtTest1",
    "resym_test::PureVirtualClassSpecialized",
    "resym_test::InterfaceImplClass",
    "resym_test::SpecializedInterfaceImplClass",
    "resym_test::ClassWithRefsAndStaticsTest",
    "resym_test::BigOffsetsStruct",
    "resym_test::BitFieldsTest3",
    "resym_test::BitFieldsTest4",
    "resym_test::BitFieldsTest5",
    "resym_test::BitFieldsTest6",
    "resym_test::BitFieldsTest7",
    "resym_test::NestedStructUnionRegression1",
    "resym_test::NtdllRegression1",
];

#[test]
fn test_type_reconstruction_portable_access_specifiers() {
    test_type_reconstruction_internal(
        "type_reconstruction_portable_access_specifiers",
        PrimitiveReconstructionFlavor::Portable,
        false,
        true,
        false,
    );
}

#[test]
fn test_type_reconstruction_microsoft_access_specifiers() {
    test_type_reconstruction_internal(
        "type_reconstruction_microsoft_access_specifiers",
        PrimitiveReconstructionFlavor::Microsoft,
        false,
        true,
        false,
    );
}

#[test]
fn test_type_reconstruction_raw_access_specifiers() {
    test_type_reconstruction_internal(
        "type_reconstruction_raw_access_specifiers",
        PrimitiveReconstructionFlavor::Raw,
        false,
        true,
        false,
    );
}

#[test]
fn test_type_reconstruction_msvc_access_specifiers() {
    test_type_reconstruction_internal(
        "type_reconstruction_msvc_access_specifiers",
        PrimitiveReconstructionFlavor::Msvc,
        false,
        true,
        false,
    );
}

fn test_type_reconstruction_internal(
    test_name: &str,
    primitives_flavor: PrimitiveReconstructionFlavor,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
    ignore_std_types: bool,
) {
    let pdb_file = PdbFile::load_from_file(Path::new(TEST_PDB_FILE_PATH)).expect("load test.pdb");
    for (i, test_case_type_name) in TEST_CASES.iter().enumerate() {
        let (reconstructed_type, _) = pdb_file
            .reconstruct_type_by_name(
                test_case_type_name,
                primitives_flavor,
                reconstruct_dependencies,
                print_access_specifiers,
                ignore_std_types,
            )
            .unwrap_or_else(|_| panic!("reconstruct type: {test_case_type_name}"));

        let snapshot_name = format!("{test_name}-{i}");
        insta::assert_snapshot!(snapshot_name, reconstructed_type);
    }
}
