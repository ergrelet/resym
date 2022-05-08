use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};
use structopt::StructOpt;
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, Style},
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

use resym::{
    backend::{Backend, BackendCommand, PDBSlot},
    frontend::{FrontendCommand, FrontendController},
    syntax_highlighting::{self, CodeTheme},
};

/// Slot for the single PDB or for the PDB we're diffing from
const PDB_MAIN_SLOT: PDBSlot = 0;
/// Slot used for the PDB we're diffing to
const PDB_DIFF_TO_SLOT: PDBSlot = 1;

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
            highlight_syntax,
        } => app.dump_types_command(
            pdb_path,
            type_name,
            print_header,
            print_dependencies,
            print_access_specifiers,
            highlight_syntax,
            output_file_path,
        ),
        ResymOptions::Diff {
            from_pdb_path,
            to_pdb_path,
            type_name,
            output_file_path,
            print_dependencies,
            print_access_specifiers,
            highlight_syntax,
            print_line_numbers,
        } => app.diff_type_command(
            from_pdb_path,
            to_pdb_path,
            type_name,
            print_dependencies,
            print_access_specifiers,
            highlight_syntax,
            print_line_numbers,
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
        /// Highlight C++ output
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
    },
    /// Compute diff for a type from the given PDB files
    Diff {
        /// Path of the PDB file to compute the diff from
        from_pdb_path: PathBuf,
        /// Path of the PDB file to compute the diff to
        to_pdb_path: PathBuf,
        /// Name of the type to diff
        type_name: String,
        /// Path of the output file
        output_file_path: Option<PathBuf>,
        /// Print declarations of referenced types
        #[structopt(short = "d", long)]
        print_dependencies: bool,
        /// Print C++ access specifiers
        #[structopt(short = "a", long)]
        print_access_specifiers: bool,
        /// Highlight C++ output and add/deleted lines
        #[structopt(short = "H", long)]
        highlight_syntax: bool,
        /// Print line numbers
        #[structopt(short = "l", long)]
        print_line_numbers: bool,
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
            .send_command(BackendCommand::LoadPDB(PDB_MAIN_SLOT, pdb_path))?;
        // Queue a request for the backend to return the list of types that
        // match the given filter
        self.backend.send_command(BackendCommand::UpdateTypeFilter(
            PDB_MAIN_SLOT,
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

    #[allow(clippy::too_many_arguments)]
    fn dump_types_command(
        &self,
        pdb_path: PathBuf,
        type_name: String,
        print_header: bool,
        print_dependencies: bool,
        print_access_specifiers: bool,
        highlight_syntax: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDB
        self.backend
            .send_command(BackendCommand::LoadPDB(PDB_MAIN_SLOT, pdb_path))?;
        // Queue a request for the backend to reconstruct the given type
        self.backend
            .send_command(BackendCommand::ReconstructTypeByName(
                PDB_MAIN_SLOT,
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
            } else if highlight_syntax {
                const LANGUAGE_SYNTAX: &str = "cpp";
                let theme = syntax_highlighting::CodeTheme::dark();
                if let Some(colorized_reconstructed_type) =
                    highlight_code(&theme, &reconstructed_type, LANGUAGE_SYNTAX)
                {
                    println!("{}", colorized_reconstructed_type);
                }
            } else {
                println!("{}", reconstructed_type);
            }
            Ok(())
        } else {
            Err(anyhow!("Invalid response received from the backend?"))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn diff_type_command(
        &self,
        from_pdb_path: PathBuf,
        to_pdb_path: PathBuf,
        type_name: String,
        print_dependencies: bool,
        print_access_specifiers: bool,
        highlight_syntax: bool,
        print_line_numbers: bool,
        output_file_path: Option<PathBuf>,
    ) -> Result<()> {
        // Request the backend to load the PDBs
        self.backend
            .send_command(BackendCommand::LoadPDB(PDB_MAIN_SLOT, from_pdb_path))?;
        self.backend
            .send_command(BackendCommand::LoadPDB(PDB_DIFF_TO_SLOT, to_pdb_path))?;

        // Queue a request for the backend to diff the given type
        self.backend.send_command(BackendCommand::DiffTypeByName(
            PDB_MAIN_SLOT,
            PDB_DIFF_TO_SLOT,
            type_name,
            print_dependencies,
            print_access_specifiers,
            print_line_numbers,
        ))?;

        // Wait for the backend to finish
        if let FrontendCommand::UpdateReconstructedType(reconstructed_type) =
            self.frontend_controller.rx_ui.recv()?
        {
            // Dump output
            if let Some(output_file_path) = output_file_path {
                let mut output_file = File::create(output_file_path)?;
                output_file.write_all(reconstructed_type.as_bytes())?;
            } else if highlight_syntax {
                const LANGUAGE_SYNTAX: &str = "cpp";
                let theme = syntax_highlighting::CodeTheme::dark();
                if let Some(colorized_reconstructed_type) =
                    highlight_code(&theme, &reconstructed_type, LANGUAGE_SYNTAX)
                {
                    println!("{}", colorized_reconstructed_type);
                }
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

/// Function relying on `syntect` to highlight the given `code` str.
/// In case of success, the result is a `String` that is ready to be printed in a
/// terminal.
fn highlight_code(theme: &CodeTheme, code: &str, language: &str) -> Option<String> {
    let highlighter = CodeHighlighter::default();
    highlighter.highlight(theme, code, language)
}

struct CodeHighlighter {
    ps: syntect::parsing::SyntaxSet,
    ts: syntect::highlighting::ThemeSet,
}

impl Default for CodeHighlighter {
    fn default() -> Self {
        Self {
            ps: syntect::parsing::SyntaxSet::load_defaults_newlines(),
            ts: syntect::highlighting::ThemeSet::load_defaults(),
        }
    }
}

impl CodeHighlighter {
    fn highlight(&self, theme: &CodeTheme, code: &str, language: &str) -> Option<String> {
        use std::fmt::Write;

        let syntax = self
            .ps
            .find_syntax_by_name(language)
            .or_else(|| self.ps.find_syntax_by_extension(language))?;

        let theme = theme.syntect_theme.syntect_key_name();
        let mut output = String::default();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme]);
        for line in LinesWithEndings::from(code) {
            let mut regions = h.highlight(line, &self.ps);
            hightlight_regions_diff(&mut regions);
            let _r = write!(
                &mut output,
                "{}",
                as_24_bit_terminal_escaped(&regions[..], true)
            );
        }

        Some(output)
    }
}

/// Changes the background of regions that have been affected in the diff.
// FIXME: This is really dirty, do better.
fn hightlight_regions_diff(regions: &mut Vec<(Style, &str)>) {
    const COLOR_TRANSPARENT: Color = Color {
        r: 0x00,
        g: 0x00,
        b: 0x00,
        a: 0x00,
    };
    const COLOR_RED: Color = Color {
        r: 0x50,
        g: 0x10,
        b: 0x10,
        a: 0xFF,
    };
    const COLOR_GREEN: Color = Color {
        r: 0x10,
        g: 0x50,
        b: 0x10,
        a: 0xFF,
    };

    let mut bg_color = COLOR_TRANSPARENT;
    regions.iter_mut().for_each(|(style, s)| {
        if *s == "+" {
            bg_color = COLOR_GREEN;
        } else if *s == "-" {
            bg_color = COLOR_RED;
        } else if *s == "\n" {
            bg_color = COLOR_TRANSPARENT;
        }
        style.background = bg_color;
    });
}
