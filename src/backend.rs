use anyhow::Result;
use eframe::epi;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::sync::mpsc::{Receiver, Sender};

use super::{UICommand, PKG_NAME, PKG_VERSION};
use crate::pdb_file::PdbFile;

pub enum WorkerCommand {
    Initialize(epi::Frame),
    /// Load a PDB file given its path as a `String`.
    LoadPDB(String),
    ReconstructType(pdb::TypeIndex, bool, bool),
    UpdateSymbolFilter(String),
}

pub struct WorkerThreadContext<'p> {
    ui_frame: Option<epi::Frame>,
    pdb_file: Option<PdbFile<'p>>,
}

impl<'p> Default for WorkerThreadContext<'p> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'p> WorkerThreadContext<'p> {
    pub fn new() -> Self {
        Self {
            ui_frame: None,
            pdb_file: None,
        }
    }

    pub fn run(
        &mut self,
        rx_worker: Receiver<WorkerCommand>,
        tx_ui: Sender<UICommand>,
    ) -> Result<()> {
        while let Ok(command) = rx_worker.recv() {
            match command {
                WorkerCommand::Initialize(ui_frame) => {
                    self.ui_frame = Some(ui_frame);
                }

                WorkerCommand::LoadPDB(file_path) => {
                    log::info!("Loading a new PDB file ...");
                    if let Err(err) = self.load_pdb_file(&file_path) {
                        log::error!("Failed to load PDB file: {}", err);
                    } else {
                        log::info!("'{}' has been loaded successfully!", file_path);
                    }
                }

                WorkerCommand::ReconstructType(
                    type_index,
                    print_header,
                    reconstruct_dependencies,
                ) => {
                    if let Some(pdb_file) = self.pdb_file.as_mut() {
                        match pdb_file
                            .reconstruct_type_by_type_index(type_index, reconstruct_dependencies)
                        {
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

                                self.send_command_to_ui(
                                    &tx_ui,
                                    UICommand::UpdateReconstructedType(reconstructed_type),
                                )?;
                            }
                            Err(err) => {
                                // Make it obvious an error occured
                                self.send_command_to_ui(
                                    &tx_ui,
                                    UICommand::UpdateReconstructedType(format!("Error: {}", err)),
                                )?;
                            }
                        }
                    }
                }

                WorkerCommand::UpdateSymbolFilter(search_filter) => {
                    if let Some(pdb_file) = self.pdb_file.as_ref() {
                        let filter_start = std::time::Instant::now();
                        let mut filtered_symbol_list: Vec<(String, pdb::TypeIndex)> =
                            if search_filter.is_empty() {
                                // No need to filter
                                pdb_file.complete_type_list.clone()
                            } else {
                                pdb_file
                                    .complete_type_list
                                    .par_iter()
                                    .filter(|r| r.0.contains(&search_filter))
                                    .cloned()
                                    .collect()
                            };
                        // Order types by type index, so the order is deterministic
                        // (i.e., independent from DashMap's hash function)
                        filtered_symbol_list.sort_by(|lhs, rhs| lhs.1.cmp(&rhs.1));
                        log::debug!(
                            "Symbol filtering took {} ms",
                            filter_start.elapsed().as_millis()
                        );
                        self.send_command_to_ui(
                            &tx_ui,
                            UICommand::UpdateFilteredSymbols(filtered_symbol_list),
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    fn send_command_to_ui(&self, tx_ui: &Sender<UICommand>, command: UICommand) -> Result<()> {
        tx_ui.send(command)?;
        // Force the UI backend to call our app's update function on the other end
        if let Some(ui_frame) = &self.ui_frame {
            ui_frame.request_repaint();
        }
        Ok(())
    }

    fn load_pdb_file(&mut self, pdb_file_path: &str) -> Result<()> {
        self.pdb_file = Some(PdbFile::load_from_file(pdb_file_path)?);
        Ok(())
    }
}
