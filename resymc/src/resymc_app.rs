use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use resym_core::{
    backend::{Backend, BackendCommand, PDBSlot},
    frontend::FrontendCommand,
    pdb_types::PrimitiveReconstructionFlavor,
    syntax_highlighting::CodeTheme,
};

use crate::{frontend::CLIFrontendController, syntax_highlighting::highlight_code};

/// Slot for the single PDB or for the PDB we're diffing from
const PDB_MAIN_SLOT: PDBSlot = 0;
/// Slot used for the PDB we're diffing to
const PDB_DIFF_TO_SLOT: PDBSlot = 1;

/// Struct that represents our CLI application.
/// It contains the whole application's context at all time.
pub struct ResymcApp {
    frontend_controller: Arc<CLIFrontendController>,
    backend: Backend,
}

impl ResymcApp {
    pub fn new() -> Result<Self> {
        // Initialize backend
        let (tx_ui, rx_ui) = crossbeam_channel::unbounded::<FrontendCommand>();
        let frontend_controller = Arc::new(CLIFrontendController::new(tx_ui, rx_ui));
        let backend = Backend::new(frontend_controller.clone())?;

        Ok(Self {
            frontend_controller,
            backend,
        })
    }

    pub fn list_types_command(
        &self,
        pdb_path: PathBuf,
        type_name_filter: String,
        case_insensitive: bool,
        use_regex: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDBFromPath(PDB_MAIN_SLOT, pdb_path))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!("Failed to load PDB: {}", err));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to return the list of types that
        // match the given filter
        self.backend.send_command(BackendCommand::UpdateTypeFilter(
            PDB_MAIN_SLOT,
            type_name_filter,
            case_insensitive,
            use_regex,
        ))?;
        // Wait for the backend to finish filtering types
        if let FrontendCommand::UpdateFilteredTypes(type_list) =
            self.frontend_controller.rx_ui.recv()?
        {
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                for (type_name, _) in type_list {
                    writeln!(output_file, "{}", &type_name)?;
                }
            } else {
                for (type_name, _) in type_list {
                    println!("{type_name}");
                }
            }
            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dump_types_command(
        &self,
        pdb_path: PathBuf,
        type_name: Option<String>,
        primitive_types_flavor: PrimitiveReconstructionFlavor,
        print_header: bool,
        print_dependencies: bool,
        print_access_specifiers: bool,
        highlight_syntax: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDBFromPath(PDB_MAIN_SLOT, pdb_path))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!("Failed to load PDB: {}", err));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to reconstruct the given type
        if let Some(type_name) = type_name {
            self.backend
                .send_command(BackendCommand::ReconstructTypeByName(
                    PDB_MAIN_SLOT,
                    type_name,
                    primitive_types_flavor,
                    print_header,
                    print_dependencies,
                    print_access_specifiers,
                ))?;
        } else {
            self.backend
                .send_command(BackendCommand::ReconstructAllTypes(
                    PDB_MAIN_SLOT,
                    primitive_types_flavor,
                    print_header,
                    print_access_specifiers,
                ))?;
        }
        // Wait for the backend to finish filtering types
        if let FrontendCommand::ReconstructTypeResult(reconstructed_type_result) =
            self.frontend_controller.rx_ui.recv()?
        {
            let reconstructed_type = reconstructed_type_result?;
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_type.as_bytes())?;
            } else if highlight_syntax {
                let theme = CodeTheme::default();
                if let Some(colorized_reconstructed_type) =
                    highlight_code(&theme, &reconstructed_type, None)
                {
                    println!("{colorized_reconstructed_type}");
                }
            } else {
                println!("{reconstructed_type}");
            }

            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn diff_type_command(
        &self,
        from_pdb_path: PathBuf,
        to_pdb_path: PathBuf,
        type_name: String,
        primitive_types_flavor: PrimitiveReconstructionFlavor,
        print_header: bool,
        print_dependencies: bool,
        print_access_specifiers: bool,
        highlight_syntax: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the first PDB
        self.backend.send_command(BackendCommand::LoadPDBFromPath(
            PDB_MAIN_SLOT,
            from_pdb_path.clone(),
        ))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!(
                    "Failed to load PDB '{}': {}",
                    from_pdb_path.display(),
                    err
                ));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Request the backend to load the second PDB
        self.backend.send_command(BackendCommand::LoadPDBFromPath(
            PDB_DIFF_TO_SLOT,
            to_pdb_path.clone(),
        ))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!(
                    "Failed to load PDB '{}': {}",
                    to_pdb_path.display(),
                    err
                ));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to diff the given type
        self.backend.send_command(BackendCommand::DiffTypeByName(
            PDB_MAIN_SLOT,
            PDB_DIFF_TO_SLOT,
            type_name,
            primitive_types_flavor,
            print_header,
            print_dependencies,
            print_access_specifiers,
        ))?;
        // Wait for the backend to finish
        if let FrontendCommand::DiffResult(reconstructed_type_diff_result) =
            self.frontend_controller.rx_ui.recv()?
        {
            let reconstructed_type_diff = reconstructed_type_diff_result?;
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_type_diff.data.as_bytes())?;
            } else if highlight_syntax {
                let theme = CodeTheme::default();
                let line_descriptions =
                    reconstructed_type_diff
                        .metadata
                        .iter()
                        .fold(vec![], |mut acc, e| {
                            acc.push(e.1);
                            acc
                        });
                if let Some(colorized_reconstructed_type) = highlight_code(
                    &theme,
                    &reconstructed_type_diff.data,
                    Some(line_descriptions),
                ) {
                    println!("{colorized_reconstructed_type}");
                }
            } else {
                println!("{}", reconstructed_type_diff.data);
            }

            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    pub fn list_modules_command(
        &self,
        pdb_path: PathBuf,
        module_path_filter: String,
        case_insensitive: bool,
        use_regex: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDBFromPath(PDB_MAIN_SLOT, pdb_path))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!("Failed to load PDB: {}", err));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to return the list of all modules
        self.backend.send_command(BackendCommand::ListModules(
            PDB_MAIN_SLOT,
            module_path_filter,
            case_insensitive,
            use_regex,
        ))?;
        // Wait for the backend to finish listing modules
        if let FrontendCommand::UpdateModuleList(module_list_result) =
            self.frontend_controller.rx_ui.recv()?
        {
            // Dump output
            let module_list = module_list_result?;
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                for (module_path, module_id) in module_list {
                    writeln!(output_file, "Mod {module_id:04} | '{module_path}'")?;
                }
            } else {
                for (module_path, module_id) in module_list {
                    println!("Mod {module_id:04} | '{module_path}'");
                }
            }

            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    pub fn dump_module_command(
        &self,
        pdb_path: PathBuf,
        module_id: usize,
        primitive_types_flavor: PrimitiveReconstructionFlavor,
        print_header: bool,
        highlight_syntax: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDBFromPath(PDB_MAIN_SLOT, pdb_path))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!("Failed to load PDB: {}", err));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to reconstruct the given module
        self.backend
            .send_command(BackendCommand::ReconstructModuleByIndex(
                PDB_MAIN_SLOT,
                module_id,
                primitive_types_flavor,
                print_header,
            ))?;
        // Wait for the backend to finish filtering types
        if let FrontendCommand::ReconstructModuleResult(reconstructed_module) =
            self.frontend_controller.rx_ui.recv()?
        {
            let reconstructed_module = reconstructed_module?;
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_module.as_bytes())?;
            } else if highlight_syntax {
                let theme = CodeTheme::default();
                if let Some(colorized_reconstructed_type) =
                    highlight_code(&theme, &reconstructed_module, None)
                {
                    println!("{colorized_reconstructed_type}");
                }
            } else {
                println!("{reconstructed_module}");
            }
            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn diff_module_command(
        &self,
        from_pdb_path: PathBuf,
        to_pdb_path: PathBuf,
        module_path: String,
        primitive_types_flavor: PrimitiveReconstructionFlavor,
        print_header: bool,
        highlight_syntax: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the first PDB
        self.backend.send_command(BackendCommand::LoadPDBFromPath(
            PDB_MAIN_SLOT,
            from_pdb_path.clone(),
        ))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!(
                    "Failed to load PDB '{}': {}",
                    from_pdb_path.display(),
                    err
                ));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Request the backend to load the second PDB
        self.backend.send_command(BackendCommand::LoadPDBFromPath(
            PDB_DIFF_TO_SLOT,
            to_pdb_path.clone(),
        ))?;
        // Wait for the backend to finish loading the PDB
        if let FrontendCommand::LoadPDBResult(result) = self.frontend_controller.rx_ui.recv()? {
            if let Err(err) = result {
                return Err(anyhow!(
                    "Failed to load PDB '{}': {}",
                    to_pdb_path.display(),
                    err
                ));
            }
        } else {
            return Err(anyhow!("Invalid response received from the backend?"));
        }

        // Queue a request for the backend to diff the given module
        self.backend.send_command(BackendCommand::DiffModuleByPath(
            PDB_MAIN_SLOT,
            PDB_DIFF_TO_SLOT,
            module_path,
            primitive_types_flavor,
            print_header,
        ))?;
        // Wait for the backend to finish
        if let FrontendCommand::DiffResult(reconstructed_module_diff_result) =
            self.frontend_controller.rx_ui.recv()?
        {
            let reconstructed_module_diff = reconstructed_module_diff_result?;
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_module_diff.data.as_bytes())?;
            } else if highlight_syntax {
                let theme = CodeTheme::default();
                let line_descriptions =
                    reconstructed_module_diff
                        .metadata
                        .iter()
                        .fold(vec![], |mut acc, e| {
                            acc.push(e.1);
                            acc
                        });
                if let Some(colorized_reconstructed_module) = highlight_code(
                    &theme,
                    &reconstructed_module_diff.data,
                    Some(line_descriptions),
                ) {
                    println!("{colorized_reconstructed_module}");
                }
            } else {
                println!("{}", reconstructed_module_diff.data);
            }

            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    use tempdir::TempDir;

    const TEST_PDB_FILE_PATH: &str = "../resym_core/tests/data/test.pdb";
    const TEST_PDB_FROM_FILE_PATH: &str = "../resym_core/tests/data/test_diff_from.pdb";
    const TEST_PDB_TO_FILE_PATH: &str = "../resym_core/tests/data/test_diff_to.pdb";

    // List types
    #[test]
    fn list_types_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::new();
        // The command should fail
        assert!(app
            .list_types_command(
                pdb_path,
                "resym_test::StructTest".to_string(),
                false,
                false,
                None,
            )
            .is_err());
    }

    #[test]
    fn list_types_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        // The command should succeed
        assert!(app
            .list_types_command(
                pdb_path,
                "resym_test::StructTest".to_string(),
                true,
                true,
                None,
            )
            .is_ok());
    }

    #[test]
    fn list_types_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        let tmp_dir =
            TempDir::new("list_types_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");
        // The command should succeed
        assert!(app
            .list_types_command(
                pdb_path,
                "resym_test::ClassWithNestedDeclarationsTest".to_string(),
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!(
                "resym_test::ClassWithNestedDeclarationsTest::NestEnum\n",
                "resym_test::ClassWithNestedDeclarationsTest\n",
                "resym_test::ClassWithNestedDeclarationsTest::NestedUnion\n",
                "resym_test::ClassWithNestedDeclarationsTest::NestedClass\n",
                "resym_test::ClassWithNestedDeclarationsTest::NestedStruct\n"
            )
        );
    }

    // Dump types
    #[test]
    fn dump_types_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::new();
        // The command should fail
        assert!(app
            .dump_types_command(
                pdb_path,
                None,
                PrimitiveReconstructionFlavor::Microsoft,
                false,
                false,
                false,
                false,
                None
            )
            .is_err());
    }

    #[test]
    fn dump_types_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);

        // The command should succeed
        assert!(app
            .dump_types_command(
                pdb_path,
                None,
                PrimitiveReconstructionFlavor::Microsoft,
                true,
                true,
                true,
                true,
                None
            )
            .is_ok());
    }

    #[test]
    fn dump_types_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        let tmp_dir =
            TempDir::new("dump_types_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");

        // The command should succeed
        assert!(app
            .dump_types_command(
                pdb_path,
                Some("resym_test::ClassWithNestedDeclarationsTest".to_string()),
                PrimitiveReconstructionFlavor::Microsoft,
                false,
                false,
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!("\nclass resym_test::ClassWithNestedDeclarationsTest { /* Size=0x1 */\n};\n")
        );
    }

    // Diff type
    #[test]
    fn diff_type_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::new();
        let pdb_path_to = PathBuf::new();

        // The command should fail
        assert!(app
            .diff_type_command(
                pdb_path_from,
                pdb_path_to,
                "".to_string(),
                PrimitiveReconstructionFlavor::Microsoft,
                false,
                false,
                false,
                false,
                None
            )
            .is_err());
    }
    #[test]
    fn diff_type_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FROM_FILE_PATH);
        let pdb_path_to = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_TO_FILE_PATH);

        // The command should succeed
        assert!(app
            .diff_type_command(
                pdb_path_from,
                pdb_path_to,
                "UserStructAddAndReplace".to_string(),
                PrimitiveReconstructionFlavor::Microsoft,
                true,
                true,
                true,
                true,
                None
            )
            .is_ok());
    }

    #[test]
    fn diff_type_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FROM_FILE_PATH);
        let pdb_path_to = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_TO_FILE_PATH);

        let tmp_dir =
            TempDir::new("diff_type_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");

        // The command should succeed
        assert!(app
            .diff_type_command(
                pdb_path_from,
                pdb_path_to,
                "UserStructAddAndReplace".to_string(),
                PrimitiveReconstructionFlavor::Portable,
                false,
                false,
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!(
                " \n-struct UserStructAddAndReplace { /* Size=0x10 */\n",
                "-  /* 0x0000 */ int32_t field1;\n-  /* 0x0004 */ char field2;\n",
                "-  /* 0x0008 */ void* field3;\n+struct UserStructAddAndReplace { /* Size=0x28 */\n",
                "+  /* 0x0000 */ int32_t before1;\n+  /* 0x0004 */ int32_t field1;\n",
                "+  /* 0x0008 */ int32_t between12;\n+  /* 0x000c */ char field2;\n",
                "+  /* 0x0010 */ int32_t between23;\n+  /* 0x0018 */ void* field3;\n",
                "+  /* 0x0020 */ int32_t after3;\n };\n",
            )
        );
    }

    // List modules
    #[test]
    fn list_modules_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::new();
        // The command should fail
        assert!(app
            .list_modules_command(pdb_path, "*".to_string(), false, false, None)
            .is_err());
    }

    #[test]
    fn list_modules_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        // The command should succeed
        assert!(app
            .list_modules_command(pdb_path, "*".to_string(), true, true, None)
            .is_ok());
    }

    #[test]
    fn list_modules_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        let tmp_dir =
            TempDir::new("list_modules_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");
        // The command should succeed
        assert!(app
            .list_modules_command(
                pdb_path,
                "*".to_string(),
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!(
                "Mod 0048 | '* Linker Generated Manifest RES *'\n",
                "Mod 0053 | '* Linker *'\n"
            )
        );
    }

    // Dump module
    #[test]
    fn dump_module_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::new();
        // The command should fail
        assert!(app
            .dump_module_command(
                pdb_path,
                9, // exe_main.obj
                PrimitiveReconstructionFlavor::Microsoft,
                false,
                false,
                None
            )
            .is_err());
    }

    #[test]
    fn dump_module_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        // The command should succeed
        assert!(app
            .dump_module_command(
                pdb_path,
                9, // exe_main.obj
                PrimitiveReconstructionFlavor::Microsoft,
                true,
                true,
                None
            )
            .is_ok());
    }

    #[test]
    fn dump_module_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FILE_PATH);
        let tmp_dir =
            TempDir::new("dump_module_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");
        // The command should succeed
        assert!(app
            .dump_module_command(
                pdb_path,
                27, // default_local_stdio_options.obj
                PrimitiveReconstructionFlavor::Portable,
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!(
                "using namespace std;\n",
                "using PUWSTR_C = const wchar_t*;\n",
                "using TP_CALLBACK_ENVIRON_V3 = _TP_CALLBACK_ENVIRON_V3;\n",
                "uint64_t* (__local_stdio_scanf_options)(); // CodeSize=8\n",
                "uint64_t _OptionsStorage;\n",
                "void (__scrt_initialize_default_local_stdio_options)(); // CodeSize=69\n",
            )
        );
    }

    // Diff module
    #[test]
    fn diff_module_command_invalid_pdb_path() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::new();
        let pdb_path_to = PathBuf::new();

        // The command should fail
        assert!(app
            .diff_module_command(
                pdb_path_from,
                pdb_path_to,
                "d:\\a01\\_work\\43\\s\\Intermediate\\vctools\\msvcrt.nativeproj_607447030\\objd\\amd64\\exe_main.obj".to_string(),
                PrimitiveReconstructionFlavor::Microsoft,
                false,
                false,
                None
            )
            .is_err());
    }

    #[test]
    fn diff_module_command_stdio_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FROM_FILE_PATH);
        let pdb_path_to = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_TO_FILE_PATH);

        // The command should succeed
        assert!(app
            .diff_module_command(
                pdb_path_from,
                pdb_path_to,
                "d:\\a01\\_work\\43\\s\\Intermediate\\vctools\\msvcrt.nativeproj_607447030\\objd\\amd64\\exe_main.obj".to_string(),
                PrimitiveReconstructionFlavor::Microsoft,
                true,
                true,
                None
            )
            .is_ok());
    }

    #[test]
    fn diff_module_command_file_successful() {
        let app = ResymcApp::new().expect("ResymcApp creation failed");
        let pdb_path_from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_FROM_FILE_PATH);
        let pdb_path_to = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TEST_PDB_TO_FILE_PATH);

        let tmp_dir =
            TempDir::new("diff_module_command_file_successful").expect("TempDir creation failed");
        let output_path = tmp_dir.path().join("output.txt");

        // The command should succeed
        assert!(app
            .diff_module_command(
                pdb_path_from,
                pdb_path_to,
                "d:\\a01\\_work\\43\\s\\Intermediate\\vctools\\msvcrt.nativeproj_607447030\\objd\\amd64\\default_local_stdio_options.obj".to_string(),
                PrimitiveReconstructionFlavor::Portable,
                false,
                false,
                Some(output_path.clone()),
            )
            .is_ok());

        // Check output file's content
        let output = fs::read_to_string(output_path).expect("Failed to read output file");
        assert_eq!(
            output,
            concat!(
                " using namespace std;\n",
                " using PUWSTR_C = const wchar_t*;\n",
                " using TP_CALLBACK_ENVIRON_V3 = _TP_CALLBACK_ENVIRON_V3;\n",
                " uint64_t* (__local_stdio_scanf_options)(); // CodeSize=8\n",
                " uint64_t _OptionsStorage;\n",
                " void (__scrt_initialize_default_local_stdio_options)(); // CodeSize=69\n",
            )
        );
    }
}
