use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    ThreadPool,
};

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crate::{
    diffing::diff_type_by_name, frontend::FrontendCommand, frontend::FrontendController,
    pdb_file::PdbFile, PKG_NAME, PKG_VERSION,
};

pub type PDBSlot = usize;

pub enum BackendCommand {
    /// Load a PDB file given its path as a `PathBuf`.
    LoadPDB(PDBSlot, PathBuf),
    /// Reconstruct a type given its type index.
    ReconstructTypeByIndex(PDBSlot, pdb::TypeIndex, bool, bool, bool),
    /// Reconstruct a type given its name.
    ReconstructTypeByName(PDBSlot, String, bool, bool, bool),
    /// Retrieve a list of types that match the given filter.
    UpdateTypeFilter(PDBSlot, String, bool, bool),
    /// Reconstruct a diff of a type given its name.
    DiffTypeByName(PDBSlot, PDBSlot, String, bool, bool, bool),
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
    let mut pdb_files: BTreeMap<PDBSlot, PdbFile> = BTreeMap::new();
    while let Ok(command) = rx_worker.recv() {
        match command {
            BackendCommand::LoadPDB(pdb_slot, pdb_file_path) => {
                log::info!("Loading a new PDB file ...");
                match PdbFile::load_from_file(&pdb_file_path) {
                    Err(err) => log::error!("Failed to load PDB file: {}", err),
                    Ok(loaded_pdb_file) => {
                        pdb_files.insert(pdb_slot, loaded_pdb_file);
                        log::info!(
                            "'{}' has been loaded successfully!",
                            pdb_file_path.display()
                        );
                    }
                }
            }

            BackendCommand::ReconstructTypeByIndex(
                pdb_slot,
                type_index,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let reconstructed_type = reconstruct_type_by_index_command(
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

            BackendCommand::ReconstructTypeByName(
                pdb_slot,
                type_name,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let reconstructed_type = reconstruct_type_by_name_command(
                        pdb_file,
                        &type_name,
                        print_header,
                        reconstruct_dependencies,
                        print_access_specifiers,
                    );
                    frontend_controller.send_command(FrontendCommand::UpdateReconstructedType(
                        reconstructed_type,
                    ))?;
                }
            }

            BackendCommand::UpdateTypeFilter(
                pdb_slot,
                search_filter,
                case_insensitive_search,
                use_regex,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
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

            BackendCommand::DiffTypeByName(
                pdb_from_slot,
                pdb_to_slot,
                type_name,
                reconstruct_dependencies,
                print_access_specifiers,
                print_line_numbers,
            ) => {
                if let Some(pdb_file_from) = pdb_files.get(&pdb_from_slot) {
                    if let Some(pdb_file_to) = pdb_files.get(&pdb_to_slot) {
                        let diffed_type = diff_type_by_name(
                            pdb_file_from,
                            pdb_file_to,
                            &type_name,
                            reconstruct_dependencies,
                            print_access_specifiers,
                            print_line_numbers,
                        );
                        frontend_controller
                            .send_command(FrontendCommand::UpdateReconstructedType(diffed_type))?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn reconstruct_type_by_index_command(
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
                let file_header = generate_file_header(pdb_file, true);
                format!("{}{}", file_header, data)
            } else {
                data
            }
        }
    }
}

fn reconstruct_type_by_name_command(
    pdb_file: &PdbFile,
    type_name: &str,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> String {
    match pdb_file.reconstruct_type_by_name(
        type_name,
        reconstruct_dependencies,
        print_access_specifiers,
    ) {
        Err(err) => {
            // Make it obvious an error occured
            format!("Error: {}", err)
        }
        Ok(data) => {
            if print_header {
                let file_header = generate_file_header(pdb_file, true);
                format!("{}{}", file_header, data)
            } else {
                data
            }
        }
    }
}

fn generate_file_header(pdb_file: &PdbFile, include_stdint: bool) -> String {
    format!(
        concat!(
            "//\n",
            "// PDB file: {}\n",
            "// Image architecture: {}\n",
            "//\n",
            "// Information extracted with {} v{}\n",
            "//\n",
            "{}"
        ),
        pdb_file.file_path.display(),
        pdb_file.machine_type,
        PKG_NAME,
        PKG_VERSION,
        if include_stdint {
            "\n#include <cstdint>\n"
        } else {
            ""
        }
    )
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
