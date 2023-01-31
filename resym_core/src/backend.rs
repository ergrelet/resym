use crossbeam_channel::{Receiver, Sender};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    slice::ParallelSliceMut,
    ThreadPool,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    diffing::diff_type_by_name,
    error::{Result, ResymCoreError},
    frontend::FrontendCommand,
    frontend::FrontendController,
    pdb_file::PdbFile,
    pdb_types::{include_headers_for_flavor, PrimitiveReconstructionFlavor},
    PKG_VERSION,
};

pub type PDBSlot = usize;

pub enum BackendCommand {
    /// Load a PDB file given its path as a `PathBuf`.
    LoadPDB(PDBSlot, PathBuf),
    /// Unload a PDB file given its slot.
    UnloadPDB(PDBSlot),
    /// Reconstruct a type given its type index for a given PDB.
    ReconstructTypeByIndex(
        PDBSlot,
        pdb::TypeIndex,
        PrimitiveReconstructionFlavor,
        bool,
        bool,
        bool,
    ),
    /// Reconstruct a type given its name for a given PDB.
    ReconstructTypeByName(
        PDBSlot,
        String,
        PrimitiveReconstructionFlavor,
        bool,
        bool,
        bool,
    ),
    /// Retrieve a list of types that match the given filter for a given PDB.
    UpdateTypeFilter(PDBSlot, String, bool, bool),
    /// Retrieve a list of types that match the given filter for multiple PDBs
    /// and merge the result.
    UpdateTypeFilterMerged(Vec<PDBSlot>, String, bool, bool),
    /// Reconstruct a diff of a type given its name.
    DiffTypeByName(
        PDBSlot,
        PDBSlot,
        String,
        PrimitiveReconstructionFlavor,
        bool,
        bool,
        bool,
    ),
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
        self.tx_worker
            .send(command)
            .map_err(|err| ResymCoreError::CrossbeamError(err.to_string()))
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
                    Err(err) => frontend_controller
                        .send_command(FrontendCommand::LoadPDBResult(Err(err)))?,
                    Ok(loaded_pdb_file) => {
                        frontend_controller
                            .send_command(FrontendCommand::LoadPDBResult(Ok(pdb_slot)))?;
                        pdb_files.insert(pdb_slot, loaded_pdb_file);
                        log::info!(
                            "'{}' has been loaded successfully!",
                            pdb_file_path.display()
                        );
                    }
                }
            }

            BackendCommand::UnloadPDB(pdb_slot) => match pdb_files.remove(&pdb_slot) {
                None => {
                    log::error!("Trying to unload an inexistent PDB");
                }
                Some(pdb_file) => {
                    log::info!("'{}' has been unloaded.", pdb_file.file_path.display());
                }
            },

            BackendCommand::ReconstructTypeByIndex(
                pdb_slot,
                type_index,
                primitives_flavor,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let reconstructed_type_result = reconstruct_type_by_index_command(
                        pdb_file,
                        type_index,
                        primitives_flavor,
                        print_header,
                        reconstruct_dependencies,
                        print_access_specifiers,
                    );
                    frontend_controller.send_command(FrontendCommand::ReconstructTypeResult(
                        reconstructed_type_result,
                    ))?;
                }
            }

            BackendCommand::ReconstructTypeByName(
                pdb_slot,
                type_name,
                primitives_flavor,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let reconstructed_type_result = reconstruct_type_by_name_command(
                        pdb_file,
                        &type_name,
                        primitives_flavor,
                        print_header,
                        reconstruct_dependencies,
                        print_access_specifiers,
                    );
                    frontend_controller.send_command(FrontendCommand::ReconstructTypeResult(
                        reconstructed_type_result,
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
                        true,
                    );
                    frontend_controller
                        .send_command(FrontendCommand::UpdateFilteredTypes(filtered_type_list))?;
                }
            }

            BackendCommand::UpdateTypeFilterMerged(
                pdb_slots,
                search_filter,
                case_insensitive_search,
                use_regex,
            ) => {
                let mut filtered_type_set = BTreeSet::default();
                for pdb_slot in pdb_slots {
                    if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                        let filtered_type_list = update_type_filter_command(
                            pdb_file,
                            &search_filter,
                            case_insensitive_search,
                            use_regex,
                            false,
                        );
                        filtered_type_set.extend(filtered_type_list.into_iter().map(|(s, _)| {
                            // Collapse all type indices to `default`. When merging
                            // type lists, we can only count on type names to
                            // represent the types.
                            (s, pdb::TypeIndex::default())
                        }));
                    }
                }
                frontend_controller.send_command(FrontendCommand::UpdateFilteredTypes(
                    filtered_type_set.into_iter().collect(),
                ))?;
            }

            BackendCommand::DiffTypeByName(
                pdb_from_slot,
                pdb_to_slot,
                type_name,
                primitives_flavor,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file_from) = pdb_files.get(&pdb_from_slot) {
                    if let Some(pdb_file_to) = pdb_files.get(&pdb_to_slot) {
                        let type_diff_result = diff_type_by_name(
                            pdb_file_from,
                            pdb_file_to,
                            &type_name,
                            primitives_flavor,
                            print_header,
                            reconstruct_dependencies,
                            print_access_specifiers,
                        );
                        frontend_controller
                            .send_command(FrontendCommand::DiffTypeResult(type_diff_result))?;
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
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> Result<String> {
    let data = pdb_file.reconstruct_type_by_type_index(
        type_index,
        &primitives_flavor,
        reconstruct_dependencies,
        print_access_specifiers,
    )?;
    if print_header {
        let file_header = generate_file_header(pdb_file, primitives_flavor, true);
        Ok(format!("{file_header}{data}"))
    } else {
        Ok(data)
    }
}

fn reconstruct_type_by_name_command(
    pdb_file: &PdbFile,
    type_name: &str,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> Result<String> {
    let data = pdb_file.reconstruct_type_by_name(
        type_name,
        primitives_flavor,
        reconstruct_dependencies,
        print_access_specifiers,
    )?;
    if print_header {
        let file_header = generate_file_header(pdb_file, primitives_flavor, true);
        Ok(format!("{file_header}{data}"))
    } else {
        Ok(data)
    }
}

fn generate_file_header(
    pdb_file: &PdbFile,
    primitives_flavor: PrimitiveReconstructionFlavor,
    include_header_files: bool,
) -> String {
    format!(
        concat!(
            "//\n",
            "// PDB file: {}\n",
            "// Image architecture: {}\n",
            "//\n",
            "// Information extracted with resym v{}\n",
            "//\n",
            "{}"
        ),
        pdb_file.file_path.display(),
        pdb_file.machine_type,
        PKG_VERSION,
        if include_header_files {
            format!("\n{}", include_headers_for_flavor(primitives_flavor))
        } else {
            "".to_string()
        }
    )
}

fn update_type_filter_command(
    pdb_file: &PdbFile,
    search_filter: &str,
    case_insensitive_search: bool,
    use_regex: bool,
    sort_by_index: bool,
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
    if sort_by_index {
        // Order types by type index, so the order is deterministic
        // (i.e., independent from DashMap's hash function)
        filtered_type_list.par_sort_by(|lhs, rhs| lhs.1.cmp(&rhs.1));
    }

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
