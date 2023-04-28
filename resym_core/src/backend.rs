use crossbeam_channel::{Receiver, Sender};
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(feature = "rayon")]
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    slice::ParallelSliceMut,
    ThreadPool,
};

#[cfg(all(not(feature = "rayon"), not(target_arch = "wasm32")))]
use std::thread::{self, JoinHandle};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::Arc,
};
#[cfg(not(target_arch = "wasm32"))]
use std::{path::PathBuf, time::Instant};
#[cfg(all(not(feature = "rayon"), target_arch = "wasm32"))]
use wasm_thread::{self as thread, JoinHandle};

use crate::pdb_file::PDBDataSource;
use crate::{
    cond_par_iter, cond_sort_by,
    diffing::diff_type_by_name,
    error::{Result, ResymCoreError},
    frontend::FrontendCommand,
    frontend::{FrontendController, ModuleList},
    pdb_file::PdbFile,
    pdb_types::{include_headers_for_flavor, PrimitiveReconstructionFlavor},
    PKG_VERSION,
};

pub type PDBSlot = usize;

pub enum BackendCommand {
    /// Load a PDB file given its path as a `PathBuf`.
    #[cfg(not(target_arch = "wasm32"))]
    LoadPDBFromPath(PDBSlot, PathBuf),
    /// Load a PDB file given its name and content as a `Vec<u8>`.
    LoadPDBFromVec(PDBSlot, String, Vec<u8>),
    /// Load a PDB file given its name and content as an `Arc<[u8]>`.
    LoadPDBFromArray(PDBSlot, String, Arc<[u8]>),
    /// Fetch data via HTTP given its URL as a `String`.
    #[cfg(feature = "http")]
    LoadPDBFromURL(PDBSlot, String),
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
    /// Reconstruct all types found in a given PDB.
    ReconstructAllTypes(PDBSlot, PrimitiveReconstructionFlavor, bool, bool),
    /// Retrieve a list of types that match the given filter for a given PDB.
    UpdateTypeFilter(PDBSlot, String, bool, bool),
    /// Retrieve a list of types that match the given filter for multiple PDBs
    /// and merge the result.
    UpdateTypeFilterMerged(Vec<PDBSlot>, String, bool, bool),
    /// Retrieve the list of all modules in a given PDB.
    ListModules(PDBSlot),
    /// Reconstruct a module given its index for a given PDB.
    ReconstructModuleByIndex(PDBSlot, usize, PrimitiveReconstructionFlavor, bool),
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
    #[cfg(feature = "rayon")]
    _worker_thread_pool: ThreadPool,
    #[cfg(not(feature = "rayon"))]
    _worker_thread: JoinHandle<()>,
}

impl Backend {
    /// Backend creation with `rayon`
    #[cfg(feature = "rayon")]
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
            let exit_result = worker_thread_routine(rx_worker, frontend_controller.clone());
            if let Err(err) = exit_result {
                log::error!("Background thread aborted: {}", err);
            }
        });
        log::debug!("Background thread pool started");

        Ok(Self {
            tx_worker,
            _worker_thread_pool: thread_pool,
        })
    }

    /// Backend creation without `rayon`
    #[cfg(not(feature = "rayon"))]
    pub fn new(
        frontend_controller: Arc<impl FrontendController + Send + Sync + 'static>,
    ) -> Result<Self> {
        let (tx_worker, rx_worker) = crossbeam_channel::unbounded::<BackendCommand>();

        // Start a new thread
        let worker_thread = thread::spawn(move || {
            let exit_result = worker_thread_routine(rx_worker, frontend_controller.clone());
            if let Err(err) = exit_result {
                log::error!("Background thread aborted: {}", err);
            }
        });
        log::debug!("Background thread started");

        Ok(Self {
            tx_worker,
            _worker_thread: worker_thread,
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
    frontend_controller: Arc<impl FrontendController + Send + Sync + 'static>,
) -> Result<()> {
    let mut pdb_files: BTreeMap<PDBSlot, PdbFile<PDBDataSource>> = BTreeMap::new();
    while let Ok(command) = rx_worker.recv() {
        match command {
            #[cfg(not(target_arch = "wasm32"))]
            BackendCommand::LoadPDBFromPath(pdb_slot, pdb_file_path) => {
                log::info!("Loading a new PDB file ...");
                match PdbFile::load_from_file(&pdb_file_path) {
                    Err(err) => frontend_controller
                        .send_command(FrontendCommand::LoadPDBResult(Err(err)))?,
                    Ok(loaded_pdb_file) => {
                        frontend_controller
                            .send_command(FrontendCommand::LoadPDBResult(Ok(pdb_slot)))?;
                        if let Some(pdb_file) = pdb_files.insert(pdb_slot, loaded_pdb_file) {
                            log::info!("'{}' has been unloaded.", pdb_file.file_path.display());
                        }
                        log::info!(
                            "'{}' has been loaded successfully!",
                            pdb_file_path.display()
                        );
                    }
                }
            }

            BackendCommand::LoadPDBFromVec(pdb_slot, pdb_name, pdb_data) => {
                log::info!("Loading a new PDB file ...");
                match PdbFile::load_from_bytes_as_vec(pdb_name.clone(), pdb_data) {
                    Err(err) => frontend_controller
                        .send_command(FrontendCommand::LoadPDBResult(Err(err)))?,
                    Ok(loaded_pdb_file) => {
                        frontend_controller
                            .send_command(FrontendCommand::LoadPDBResult(Ok(pdb_slot)))?;
                        if let Some(pdb_file) = pdb_files.insert(pdb_slot, loaded_pdb_file) {
                            log::info!("'{}' has been unloaded.", pdb_file.file_path.display());
                        }
                        log::info!("'{}' has been loaded successfully!", pdb_name);
                    }
                }
            }

            BackendCommand::LoadPDBFromArray(pdb_slot, pdb_name, pdb_data) => {
                log::info!("Loading a new PDB file ...");
                match PdbFile::load_from_bytes_as_array(pdb_name.clone(), pdb_data) {
                    Err(err) => frontend_controller
                        .send_command(FrontendCommand::LoadPDBResult(Err(err)))?,
                    Ok(loaded_pdb_file) => {
                        frontend_controller
                            .send_command(FrontendCommand::LoadPDBResult(Ok(pdb_slot)))?;
                        if let Some(pdb_file) = pdb_files.insert(pdb_slot, loaded_pdb_file) {
                            log::info!("'{}' has been unloaded.", pdb_file.file_path.display());
                        }
                        log::info!("'{}' has been loaded successfully!", pdb_name);
                    }
                }
            }

            #[cfg(feature = "http")]
            BackendCommand::LoadPDBFromURL(pdb_slot, url) => {
                log::info!("Fetching data from URL ...");
                // Parse URL and extract file name, if any
                match url::Url::parse(&url) {
                    Err(err) => log::error!("Failed to parse URL: {err}"),
                    Ok(url) => {
                        let url_path = url.path();
                        if let Some(pdb_name) = url_path.split('/').last() {
                            let frontend_controller = frontend_controller.clone();
                            let pdb_name = pdb_name.to_string();
                            let request = ehttp::Request::get(url);
                            ehttp::fetch(request, move |result: ehttp::Result<ehttp::Response>| {
                                match result {
                                    Err(err) => frontend_controller
                                        .send_command(FrontendCommand::LoadPDBResult(Err(
                                            ResymCoreError::EHttpError(err),
                                        )))
                                        .expect("frontend unavailable"),
                                    Ok(response) => {
                                        frontend_controller
                                            .send_command(FrontendCommand::LoadURLResult(Ok((
                                                pdb_slot,
                                                pdb_name,
                                                response.bytes,
                                            ))))
                                            .expect("frontend unavailable");
                                    }
                                }
                            });
                        } else {
                            log::error!("URL doesn't point to a file");
                        }
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

            BackendCommand::ReconstructAllTypes(
                pdb_slot,
                primitives_flavor,
                print_header,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let reconstructed_type_result = reconstruct_all_types_command(
                        pdb_file,
                        primitives_flavor,
                        print_header,
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

            BackendCommand::ReconstructModuleByIndex(
                pdb_slot,
                module_index,
                primitives_flavor,
                print_header,
            ) => {
                if let Some(pdb_file) = pdb_files.get_mut(&pdb_slot) {
                    let reconstructed_module_result = reconstruct_module_by_index_command(
                        pdb_file,
                        module_index,
                        primitives_flavor,
                        print_header,
                    );
                    frontend_controller.send_command(FrontendCommand::ReconstructModuleResult(
                        reconstructed_module_result,
                    ))?;
                }
            }

            BackendCommand::ListModules(pdb_slot) => {
                if let Some(pdb_file) = pdb_files.get(&pdb_slot) {
                    let module_list = list_modules_command(pdb_file);
                    frontend_controller
                        .send_command(FrontendCommand::UpdateModuleList(module_list))?;
                }
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

fn reconstruct_type_by_index_command<'p, T>(
    pdb_file: &PdbFile<'p, T>,
    type_index: pdb::TypeIndex,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> Result<String>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
    let data = pdb_file.reconstruct_type_by_type_index(
        type_index,
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

fn reconstruct_type_by_name_command<'p, T>(
    pdb_file: &PdbFile<'p, T>,
    type_name: &str,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
) -> Result<String>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
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

fn reconstruct_all_types_command<'p, T>(
    pdb_file: &PdbFile<'p, T>,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
    print_access_specifiers: bool,
) -> Result<String>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
    let data = pdb_file.reconstruct_all_types(primitives_flavor, print_access_specifiers)?;
    if print_header {
        let file_header = generate_file_header(pdb_file, primitives_flavor, true);
        Ok(format!("{file_header}{data}"))
    } else {
        Ok(data)
    }
}

fn list_modules_command<'p, T>(pdb_file: &PdbFile<'p, T>) -> Result<ModuleList>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
    pdb_file.module_list()
}

fn reconstruct_module_by_index_command<'p, T>(
    pdb_file: &mut PdbFile<'p, T>,
    module_index: usize,
    primitives_flavor: PrimitiveReconstructionFlavor,
    print_header: bool,
) -> Result<String>
where
    T: io::Seek + io::Read + std::fmt::Debug + 'p,
{
    let data = pdb_file.reconstruct_module_by_index(module_index, &primitives_flavor)?;
    if print_header {
        let file_header = generate_file_header(pdb_file, primitives_flavor, true);
        Ok(format!("{file_header}\n{data}"))
    } else {
        Ok(data)
    }
}

fn generate_file_header<T>(
    pdb_file: &PdbFile<T>,
    primitives_flavor: PrimitiveReconstructionFlavor,
    include_header_files: bool,
) -> String
where
    T: io::Seek + io::Read,
{
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

fn update_type_filter_command<T>(
    pdb_file: &PdbFile<T>,
    search_filter: &str,
    case_insensitive_search: bool,
    use_regex: bool,
    sort_by_index: bool,
) -> Vec<(String, pdb::TypeIndex)>
where
    T: io::Seek + io::Read,
{
    let filter_start = Instant::now();

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
        cond_sort_by!(filtered_type_list, |lhs, rhs| lhs.1.cmp(&rhs.1));
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
        Ok(regex) => cond_par_iter!(type_list)
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
        cond_par_iter!(type_list)
            .filter(|r| r.0.to_lowercase().contains(&search_filter))
            .cloned()
            .collect()
    } else {
        cond_par_iter!(type_list)
            .filter(|r| r.0.contains(search_filter))
            .cloned()
            .collect()
    }
}
