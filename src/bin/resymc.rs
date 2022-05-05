use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};
use structopt::StructOpt;

use resym::{
    backend::{Backend, BackendCommand},
    frontend::{FrontendCommand, FrontendController},
};

fn main() -> Result<()> {
    let app = ResymcApp::new()?;

    // Process command and options
    let opt = ResymOptions::from_args();
    match opt {
        ResymOptions::List {
            pdb_path,
            type_name_filter,
            output_file_path,
            case_insensitive,
            use_regex,
        } => app.list_types_command(
            pdb_path,
            type_name_filter,
            case_insensitive,
            use_regex,
            output_file_path,
        ),
        ResymOptions::Dump {
            pdb_path,
            type_name,
            output_file_path,
            print_header,
            print_dependencies,
            print_access_specifiers,
        } => app.dump_types_command(
            pdb_path,
            type_name,
            print_header,
            print_dependencies,
            print_access_specifiers,
            output_file_path,
        ),
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "resymc",
    about = "resym is a utility that allows browsing and extracting types from PDB files."
)]
enum ResymOptions {
    /// List types from a given PDB file
    List {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Search filter
        type_name_filter: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Do not match case
        #[structopt(short = "i", long)]
        case_insensitive: bool,
        /// Use regular expressions
        #[structopt(short = "r", long)]
        use_regex: bool,
    },
    /// Dump type from a given PDB file
    Dump {
        /// Path to the PDB file
        pdb_path: PathBuf,
        /// Name of the type to extract
        type_name: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Print header
        #[structopt(short = "h", long)]
        print_header: bool,
        /// Print declarations of referenced types
        #[structopt(short = "d", long)]
        print_dependencies: bool,
        /// Print C++ access specifiers
        #[structopt(short = "a", long)]
        print_access_specifiers: bool,
    },
}

/// Struct that represents our CLI application.
/// It contains the whole application's context at all time.
struct ResymcApp {
    frontend_controller: Arc<CLIFrontendController>,
    backend: Backend,
}

impl ResymcApp {
    fn new() -> Result<Self> {
        // Initialize backend
        let (tx_ui, rx_ui) = crossbeam_channel::unbounded::<FrontendCommand>();
        let frontend_controller = Arc::new(CLIFrontendController::new(tx_ui, rx_ui));
        let backend = Backend::new(frontend_controller.clone())?;

        Ok(Self {
            frontend_controller,
            backend,
        })
    }

    fn list_types_command(
        &self,
        pdb_path: PathBuf,
        type_name_filter: String,
        case_insensitive: bool,
        use_regex: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDB(pdb_path))?;
        // Queue a request for the backend to return the list of types that
        // match the given filter
        self.backend.send_command(BackendCommand::UpdateTypeFilter(
            type_name_filter,
            case_insensitive,
            use_regex,
        ))?;

        // Wait for the backend to finish
        if let FrontendCommand::UpdateFilteredTypes(type_list) =
            self.frontend_controller.rx_ui.recv()?
        {
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                for (type_name, _) in type_list {
                    output_file.write_all(type_name.as_bytes())?;
                }
            } else {
                for (type_name, _) in type_list {
                    println!("{}", type_name);
                }
            }
            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    fn dump_types_command(
        &self,
        pdb_path: PathBuf,
        type_name: String,
        print_header: bool,
        print_dependencies: bool,
        print_access_specifiers: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDB(pdb_path))?;
        // Queue a request for the backend to reconstruct the given type
        self.backend
            .send_command(BackendCommand::ReconstructTypeByName(
                type_name,
                print_header,
                print_dependencies,
                print_access_specifiers,
            ))?;

        // Wait for the backend to finish
        if let FrontendCommand::UpdateReconstructedType(reconstructed_type) =
            self.frontend_controller.rx_ui.recv()?
        {
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_type.as_bytes())?;
            } else {
                println!("{}", reconstructed_type);
            }
            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }
}

/// Frontend implementation for the CLI application
/// This struct enables the backend to communicate with us (the frontend)
struct CLIFrontendController {
    tx_ui: Sender<FrontendCommand>,
    rx_ui: Receiver<FrontendCommand>,
}

impl FrontendController for CLIFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        Ok(self.tx_ui.send(command)?)
    }
}

impl CLIFrontendController {
    fn new(tx_ui: Sender<FrontendCommand>, rx_ui: Receiver<FrontendCommand>) -> Self {
        Self { tx_ui, rx_ui }
    }
}
