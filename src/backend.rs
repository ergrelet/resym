use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    ThreadPool,
};

use std::sync::Arc;

use super::{frontend::FrontendCommand, PKG_NAME, PKG_VERSION};
use crate::{frontend::FrontendController, pdb_file::PdbFile};

pub enum BackendCommand {
    /// Load a PDB file given its path as a `String`.
    LoadPDB(String),
    ReconstructType(pdb::TypeIndex, bool, bool, bool),
    UpdateSymbolFilter(String, bool, bool),
}

pub struct Backend {
    tx_worker: Sender<BackendCommand>,
    _thread_pool: ThreadPool,
}

impl Backend {
    pub fn new(
        frontend_controller: Arc<impl FrontendController + Send + Sync + 'static>,
    ) -> Result<Self> {
        let (tx_worker, rx_worker) = crossbeam_channel::unbounded::<BackendCommand>();

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
                        log::info!("'{}' has been loaded successfully!", pdb_file_path);
                    }
                }
            }

            BackendCommand::ReconstructType(
                type_index,
                print_header,
                reconstruct_dependencies,
                print_access_specifiers,
            ) => {
                if let Some(pdb_file) = pdb_file.as_mut() {
                    match pdb_file.reconstruct_type_by_type_index(
                        type_index,
                        reconstruct_dependencies,
                        print_access_specifiers,
                    ) {
                        Ok(data) => {
                            let reconstructed_type = if print_header {
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
                                    pdb_file.file_path,
                                    pdb_file.machine_type,
                                    PKG_NAME,
                                    PKG_VERSION,
                                    data
                                )
                            } else {
                                data
                            };

                            frontend_controller.send_command(
                                FrontendCommand::UpdateReconstructedType(reconstructed_type),
                            )?;
                        }
                        Err(err) => {
                            // Make it obvious an error occured
                            frontend_controller.send_command(
                                FrontendCommand::UpdateReconstructedType(format!("Error: {}", err)),
                            )?;
                        }
                    }
                }
            }

            BackendCommand::UpdateSymbolFilter(
                search_filter,
                case_insensitive_search,
                use_regex,
            ) => {
                if let Some(pdb_file) = pdb_file.as_ref() {
                    let filter_start = std::time::Instant::now();
                    let mut filtered_symbol_list: Vec<(String, pdb::TypeIndex)> =
                        if search_filter.is_empty() {
                            // No need to filter
                            pdb_file.complete_type_list.clone()
                        } else if use_regex {
                            filter_symbol_regex(
                                &pdb_file.complete_type_list,
                                &search_filter,
                                case_insensitive_search,
                            )
                        } else {
                            filter_symbol_regular(
                                &pdb_file.complete_type_list,
                                &search_filter,
                                case_insensitive_search,
                            )
                        };
                    // Order types by type index, so the order is deterministic
                    // (i.e., independent from DashMap's hash function)
                    filtered_symbol_list.sort_by(|lhs, rhs| lhs.1.cmp(&rhs.1));
                    log::debug!(
                        "Symbol filtering took {} ms",
                        filter_start.elapsed().as_millis()
                    );
                    frontend_controller.send_command(FrontendCommand::UpdateFilteredSymbols(
                        filtered_symbol_list,
                    ))?;
                }
            }
        }
    }

    Ok(())
}

fn filter_symbol_regex(
    symbol_list: &[(String, pdb::TypeIndex)],
    search_filter: &str,
    case_insensitive_search: bool,
) -> Vec<(String, pdb::TypeIndex)> {
    match regex::RegexBuilder::new(search_filter)
        .case_insensitive(case_insensitive_search)
        .build()
    {
        // In case of error, return an empty result
        Err(_) => vec![],
        Ok(regex) => symbol_list
            .par_iter()
            .filter(|r| regex.find(&r.0).is_some())
            .cloned()
            .collect(),
    }
}

fn filter_symbol_regular(
    symbol_list: &[(String, pdb::TypeIndex)],
    search_filter: &str,
    case_insensitive_search: bool,
) -> Vec<(String, pdb::TypeIndex)> {
    if case_insensitive_search {
        let search_filter = search_filter.to_lowercase();
        symbol_list
            .par_iter()
            .filter(|r| r.0.to_lowercase().contains(&search_filter))
            .cloned()
            .collect()
    } else {
        symbol_list
            .par_iter()
            .filter(|r| r.0.contains(search_filter))
            .cloned()
            .collect()
    }
}
