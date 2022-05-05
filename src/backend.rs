use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    ThreadPool,
};

use std::{path::PathBuf, sync::Arc};

use crate::{
    frontend::FrontendCommand, frontend::FrontendController, pdb_file::PdbFile, PKG_NAME,
    PKG_VERSION,
};

pub enum BackendCommand {
    /// Load a PDB file given its path as a `PathBuf`.
    LoadPDB(PathBuf),
    /// Reconstruct a type given its type index.
    ReconstructType(pdb::TypeIndex, bool, bool, bool),
    /// Retrieve a list of types that match the given filter.
    UpdateTypeFilter(String, bool, bool),
}

/// Struct that represents the backend. The backend is responsible
/// for the actual PDB processing (e.g., type listing and reconstruction).
pub struct Backend {
    tx_worker: Sender<BackendCommand>,
    _thread_pool: ThreadPool,
}

impl Backend {
    pub fn new(
        frontend_controller: Arc<impl FrontendController + Send + Sync + 'static>,
    ) -> Result<Self> {
        let (tx_worker, rx_worker) = crossbeam_channel::unbounded::<BackendCommand>();

        // Start a thread pool with as many threads as there are CPUs on the machine,
        // minus one (because we account for the GUI thread).
        // Note: Calling `num_threads` with 0 is valid.
        let cpu_count = num_cpus::get();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(cpu_count - 1)
            .build()?;
        thread_pool.spawn(move || {
            let exit_result = worker_thread_routine(rx_worker, frontend_controller.as_ref());
            if let Err(err) = exit_result {
                log::error!("Background thread aborted: {}", err);
            }
        });
        log::debug!("Background thread started");

        Ok(Self {
            tx_worker,
            _thread_pool: thread_pool,
        })
    }

    pub fn send_command(&self, command: BackendCommand) -> Result<()> {
        Ok(self.tx_worker.send(command)?)
    }
}

/// Main backend routine. This processes commands sent by the frontend and sends
/// the result back.
fn worker_thread_routine(
    rx_worker: Receiver<BackendCommand>,
    frontend_controller: &impl FrontendController,
) -> Result<()> {
    let mut pdb_file: Option<PdbFile> = None;
    while let Ok(command) = rx_worker.recv() {
        match command {
            BackendCommand::LoadPDB(pdb_file_path) => {
                log::info!("Loading a new PDB file ...");
                match PdbFile::load_from_file(&pdb_file_path) {
                    Err(err) => log::error!("Failed to load PDB file: {}", err),
                    Ok(loaded_pdb_file) => {
                        pdb_file = Some(loaded_pdb_file);
                        log::info!(
                            "'{}' has been loaded successfully!",
                            pdb_file_path.display()
                        );
                    }
                }
            }

            BackendCommand::ReconstructType(
                type_index,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_file.as_ref() {
                    let reconstructed_type = reconstruct_type_command(
                        pdb_file,
                        type_index,
                        print_header,
                        reconstruct_dependencies,
                        print_access_specifiers,
                    );
                    frontend_controller.send_command(FrontendCommand::UpdateReconstructedType(
                        reconstructed_type,
                    ))?;
                }
            }

            BackendCommand::UpdateTypeFilter(search_filter, case_insensitive_search, use_regex) => {
                if let Some(pdb_file) = pdb_file.as_ref() {
                    let filtered_type_list = update_type_filter_command(
                        pdb_file,
                        &search_filter,
                        case_insensitive_search,
                        use_regex,
                    );
                    frontend_controller
                        .send_command(FrontendCommand::UpdateFilteredTypes(filtered_type_list))?;
                }
            }
        }
    }

    Ok(())
}

fn reconstruct_type_command(
    pdb_file: &PdbFile,
    type_index: pdb::TypeIndex,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> String {
    match pdb_file.reconstruct_type_by_type_index(
        type_index,
        reconstruct_dependencies,
        print_access_specifiers,
    ) {
        Err(err) => {
            // Make it obvious an error occured
            format!("Error: {}", err)
        }
        Ok(data) => {
            if print_header {
                format!(
                    concat!(
                        "//\n",
                        "// PDB file: {}\n",
                        "// Image architecture: {}\n",
                        "//\n",
                        "// Information extracted with {} v{}\n",
                        "//\n",
                        "\n",
                        "#include <cstdint>\n",
                        "{}"
                    ),
                    pdb_file.file_path.display(),
                    pdb_file.machine_type,
                    PKG_NAME,
                    PKG_VERSION,
                    data
                )
            } else {
                data
            }
        }
    }
}

fn update_type_filter_command(
    pdb_file: &PdbFile,
    search_filter: &str,
    case_insensitive_search: bool,
    use_regex: bool,
) -> Vec<(String, pdb::TypeIndex)> {
    let filter_start = std::time::Instant::now();

    let mut filtered_type_list = if search_filter.is_empty() {
        // No need to filter
        pdb_file.complete_type_list.clone()
    } else if use_regex {
        filter_types_regex(
            &pdb_file.complete_type_list,
            search_filter,
            case_insensitive_search,
        )
    } else {
        filter_types_regular(
            &pdb_file.complete_type_list,
            search_filter,
            case_insensitive_search,
        )
    };
    // Order types by type index, so the order is deterministic
    // (i.e., independent from DashMap's hash function)
    filtered_type_list.sort_by(|lhs, rhs| lhs.1.cmp(&rhs.1));

    log::debug!(
        "Type filtering took {} ms",
        filter_start.elapsed().as_millis()
    );

    filtered_type_list
}

/// Filter type list with a regular expression
fn filter_types_regex(
    type_list: &[(String, pdb::TypeIndex)],
    search_filter: &str,
    case_insensitive_search: bool,
) -> Vec<(String, pdb::TypeIndex)> {
    match regex::RegexBuilder::new(search_filter)
        .case_insensitive(case_insensitive_search)
        .build()
    {
        // In case of error, return an empty result
        Err(_) => vec![],
        Ok(regex) => type_list
            .par_iter()
            .filter(|r| regex.find(&r.0).is_some())
            .cloned()
            .collect(),
    }
}

/// Filter type list with a plain (sub-)string
fn filter_types_regular(
    type_list: &[(String, pdb::TypeIndex)],
    search_filter: &str,
    case_insensitive_search: bool,
) -> Vec<(String, pdb::TypeIndex)> {
    if case_insensitive_search {
        let search_filter = search_filter.to_lowercase();
        type_list
            .par_iter()
            .filter(|r| r.0.to_lowercase().contains(&search_filter))
            .cloned()
            .collect()
    } else {
        type_list
            .par_iter()
            .filter(|r| r.0.contains(search_filter))
            .cloned()
            .collect()
    }
}
