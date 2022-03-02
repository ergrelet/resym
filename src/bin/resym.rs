use anyhow::Result;
use eframe::{
    egui::{self, Visuals},
    epi,
};
use egui::{ScrollArea, TextStyle};
use memory_logger::blocking::MemoryLogger;
use rayon::ThreadPool;
use tinyfiledialogs::open_file_dialog;

use std::sync::mpsc::{self, Receiver, Sender};

use resym::{
    backend::{WorkerCommand, WorkerThreadContext},
    UICommand, PKG_NAME, PKG_VERSION,
};

fn main() -> Result<()> {
    let logger = MemoryLogger::setup(log::Level::Info)?;
    let app = ResymApp::new(logger)?;
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app), native_options);
}

struct ResymApp {
    logger: &'static MemoryLogger,
    tx_worker: Sender<WorkerCommand>,
    rx_ui: Receiver<UICommand>,
    filtered_type_list: Vec<(String, pdb::TypeIndex)>,
    selected_row: usize,
    search_filter: String,
    reconstructed_type_content: String,
    console_content: String,
    _thread_pool: ThreadPool,
}

impl<'p> ResymApp {
    fn new(logger: &'static MemoryLogger) -> Result<Self> {
        let (tx_worker, rx_worker) = mpsc::channel::<WorkerCommand>();
        let (tx_ui, rx_ui) = mpsc::channel::<UICommand>();

        let cpu_count = num_cpus::get();
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(cpu_count - 1)
            .build()?;
        thread_pool.spawn(move || {
            let mut ctx = WorkerThreadContext::new();
            let worker_exit_result = ctx.run(rx_worker, tx_ui);
            if let Err(err) = worker_exit_result {
                log::error!("Background thread aborted: {}", err);
            }
        });
        log::debug!("Background thread started");

        Ok(Self {
            logger,
            tx_worker,
            rx_ui,
            filtered_type_list: vec![],
            selected_row: usize::MAX,
            search_filter: String::default(),
            reconstructed_type_content: String::default(),
            console_content: String::default(),
            _thread_pool: thread_pool,
        })
    }

    fn process_ui_commands(&mut self) {
        while let Ok(cmd) = self.rx_ui.try_recv() {
            match cmd {
                UICommand::UpdateReconstructedType(data) => {
                    self.reconstructed_type_content = data;
                }

                UICommand::UpdateFilteredSymbols(filtered_symbols) => {
                    self.filtered_type_list = filtered_symbols;
                    self.selected_row = usize::MAX;
                }
            }
        }
    }

    fn draw_menu_bar(&mut self, ui: &mut egui::Ui, frame: &epi::Frame) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open PDB file").clicked() {
                    if let Some(file_path) = open_file_dialog(
                        "Select a PDB file",
                        "",
                        Some((&["*.pdb"], "PDB files (*.pdb)")),
                    ) {
                        if let Err(err) = self.tx_worker.send(WorkerCommand::LoadPDB(file_path)) {
                            log::error!("Failed to load the PDB file: {}", err);
                        } else {
                            let result = self
                                .tx_worker
                                .send(WorkerCommand::UpdateSymbolFilter(String::default()));
                            if let Err(err) = result {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                        }
                    }
                }
                if ui.button("Exit").clicked() {
                    frame.quit();
                }
            });
        });
    }

    fn draw_symbol_list(&mut self, ui: &mut egui::Ui) {
        let num_rows = self.filtered_type_list.len();
        const TEXT_STYLE: TextStyle = TextStyle::Body;
        let row_height = ui.text_style_height(&TEXT_STYLE);
        ui.with_layout(
            egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, num_rows, |ui, row_range| {
                        for row_index in row_range {
                            let (symbol_name, type_index) = &self.filtered_type_list[row_index];

                            if ui
                                .selectable_label(self.selected_row == row_index, symbol_name)
                                .clicked()
                            {
                                self.selected_row = row_index;
                                let result = self
                                    .tx_worker
                                    .send(WorkerCommand::ReconstructType(*type_index));
                                if let Err(err) = result {
                                    log::error!("Failed to reconstruct type: {}", err);
                                }
                            }
                        }
                    });
            },
        );
    }

    fn draw_console(&mut self, ui: &mut egui::Ui) {
        // Update console
        self.console_content.push_str(&self.logger.read());
        self.logger.clear();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom()
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.console_content.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            });
    }
}

impl epi::App for ResymApp {
    fn name(&self) -> &str {
        PKG_NAME
    }

    /// Called once before the first frame.
    fn setup(
        &mut self,
        ctx: &egui::Context,
        frame: &epi::Frame,
        _storage: Option<&dyn epi::Storage>,
    ) {
        // TODO: Theme setting to switch between dark and light themes
        ctx.set_visuals(Visuals::dark());
        log::info!("{} {}", PKG_NAME, PKG_VERSION);
        // If this fails, let it burn
        self.tx_worker
            .send(WorkerCommand::Initialize(frame.clone()))
            .unwrap();
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
        // Process incoming commands, if any
        self.process_ui_commands();

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar
            self.draw_menu_bar(ui, frame);
        });

        egui::SidePanel::left("side_panel")
            .default_width(250.0)
            .width_range(100.0..=f32::INFINITY)
            .show(ctx, |ui| {
                ui.label("Search");
                ui.add_space(4.0);

                if ui.text_edit_singleline(&mut self.search_filter).changed() {
                    // Update filtered list if filter has changed
                    let result = self.tx_worker.send(WorkerCommand::UpdateSymbolFilter(
                        self.search_filter.clone(),
                    ));
                    if let Err(err) = result {
                        log::error!("Failed to update type filter value: {}", err);
                    }
                }
                ui.add_space(4.0);

                // Display list of symbol names
                self.draw_symbol_list(ui);
            });

        // Bottom panel containing the console
        egui::TopBottomPanel::bottom("bottom_panel")
            .default_height(100.0)
            .show(ctx, |ui| {
                // Console panel
                ui.vertical(|ui| {
                    ui.label("Console");
                    ui.add_space(4.0);

                    self.draw_console(ui);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.label("Reconstructed type(s) - C++");
            ui.add_space(4.0);

            // Symbol dump area
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.reconstructed_type_content.as_str())
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });
        });
    }
}
