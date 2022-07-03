#![windows_subsystem = "windows"]

mod frontend;
mod syntax_highlighting;

use anyhow::Result;
use eframe::egui::{self, ScrollArea, TextStyle};
use memory_logger::blocking::MemoryLogger;
use resym_core::{
    backend::{Backend, BackendCommand, PDBSlot},
    diffing::DiffChange,
    frontend::{FrontendCommand, TypeList},
    pdb_types::PrimitiveReconstructionFlavor,
    syntax_highlighting::CodeTheme,
};
use serde::{Deserialize, Serialize};
use tinyfiledialogs::open_file_dialog;

use std::fmt::Write;
use std::{sync::Arc, vec};

use crate::{frontend::EguiFrontendController, syntax_highlighting::highlight_code};

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Slot for the single PDB or for the PDB we're diffing from
const PDB_MAIN_SLOT: PDBSlot = 0;
/// Slot used for the PDB we're diffing to
const PDB_DIFF_SLOT: PDBSlot = 1;

fn main() -> Result<()> {
    let logger = MemoryLogger::setup(log::Level::Info)?;
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        PKG_NAME,
        native_options,
        Box::new(|cc| Box::new(ResymApp::new(cc, logger).expect("application creation"))),
    );
}

/// Struct that represents our GUI application.
/// It contains the whole application's context at all time.
struct ResymApp {
    logger: &'static MemoryLogger,
    current_mode: ResymAppMode,
    filtered_type_list: TypeList,
    selected_row: usize,
    search_filter: String,
    console_content: Vec<String>,
    settings_wnd_open: bool,
    settings: ResymAppSettings,
    frontend_controller: Arc<EguiFrontendController>,
    backend: Backend,
}

#[derive(PartialEq)]
enum ResymAppMode {
    /// Mode in which the application starts
    Idle,
    /// This mode means we're browsing a single PDB file
    Browsing(String, usize, String),
    /// This mode means we're comparing two PDB files for differences
    Comparing(String, String, usize, Vec<DiffChange>, String),
}

// GUI-related trait
impl eframe::App for ResymApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Save settings on shutdown
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Process incoming commands, if any
        self.process_ui_commands();

        // Update theme
        let theme = if self.settings.use_light_theme {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        ctx.set_visuals(theme);

        // Draw "Settings" window if open
        self.update_settings_window(ctx);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar
            self.update_menu_bar(ui, frame);
        });

        egui::SidePanel::left("side_panel")
            .default_width(250.0)
            .width_range(100.0..=f32::INFINITY)
            .show(ctx, |ui| {
                ui.label("Search");
                ui.add_space(4.0);

                if ui.text_edit_singleline(&mut self.search_filter).changed() {
                    // Update filtered list if filter has changed
                    let result = if let ResymAppMode::Comparing(..) = self.current_mode {
                        self.backend
                            .send_command(BackendCommand::UpdateTypeFilterMerged(
                                vec![PDB_MAIN_SLOT, PDB_DIFF_SLOT],
                                self.search_filter.clone(),
                                self.settings.search_case_insensitive,
                                self.settings.search_use_regex,
                            ))
                    } else {
                        self.backend.send_command(BackendCommand::UpdateTypeFilter(
                            PDB_MAIN_SLOT,
                            self.search_filter.clone(),
                            self.settings.search_case_insensitive,
                            self.settings.search_use_regex,
                        ))
                    };
                    if let Err(err) = result {
                        log::error!("Failed to update type filter value: {}", err);
                    }
                }
                ui.add_space(4.0);

                // Display list of type names
                self.update_type_list(ui);
            });

        // Bottom panel containing the console
        egui::TopBottomPanel::bottom("bottom_panel")
            .default_height(100.0)
            .show(ctx, |ui| {
                // Console panel
                ui.vertical(|ui| {
                    ui.label("Console");
                    ui.add_space(4.0);

                    self.update_console(ui);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.label(if let ResymAppMode::Comparing(..) = self.current_mode {
                "Differences between reconstructed type(s) - C++"
            } else {
                "Reconstructed type(s) - C++"
            });
            ui.add_space(4.0);

            self.update_code_view(ui);
        });
    }
}

// Utility associated functions and methods
impl<'p> ResymApp {
    fn new(cc: &eframe::CreationContext<'_>, logger: &'static MemoryLogger) -> Result<Self> {
        let (tx_ui, rx_ui) = crossbeam_channel::unbounded::<FrontendCommand>();
        let frontend_controller = Arc::new(EguiFrontendController::new(
            tx_ui,
            rx_ui,
            cc.egui_ctx.clone(),
        ));
        let backend = Backend::new(frontend_controller.clone())?;

        // Load settings on launch
        let mut settings = ResymAppSettings::default();
        if let Some(storage) = cc.storage {
            settings = eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        }

        log::info!("{} {}", PKG_NAME, PKG_VERSION);
        Ok(Self {
            logger,
            current_mode: ResymAppMode::Idle,
            filtered_type_list: vec![],
            selected_row: usize::MAX,
            search_filter: String::default(),
            console_content: vec![],
            settings_wnd_open: false,
            settings,
            frontend_controller,
            backend,
        })
    }

    fn process_ui_commands(&mut self) {
        while let Ok(cmd) = self.frontend_controller.rx_ui.try_recv() {
            match cmd {
                FrontendCommand::LoadPDBResult(result) => match result {
                    Err(err) => {
                        log::error!("Failed to load PDB file: {}", err);
                    }
                    Ok(pdb_slot) => {
                        if pdb_slot == PDB_MAIN_SLOT {
                            // Unload the PDB used for diffing if one is loaded
                            if let ResymAppMode::Comparing(..) = self.current_mode {
                                if let Err(err) = self
                                    .backend
                                    .send_command(BackendCommand::UnloadPDB(PDB_DIFF_SLOT))
                                {
                                    log::error!(
                                        "Failed to unload the PDB used for comparison: {}",
                                        err
                                    );
                                }
                            }

                            self.current_mode =
                                ResymAppMode::Browsing(String::default(), 0, String::default());
                            // Request a type list update
                            if let Err(err) =
                                self.backend.send_command(BackendCommand::UpdateTypeFilter(
                                    PDB_MAIN_SLOT,
                                    String::default(),
                                    false,
                                    false,
                                ))
                            {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                        } else if pdb_slot == PDB_DIFF_SLOT {
                            self.current_mode = ResymAppMode::Comparing(
                                String::default(),
                                String::default(),
                                0,
                                vec![],
                                String::default(),
                            );
                            // Request a type list update
                            if let Err(err) =
                                self.backend
                                    .send_command(BackendCommand::UpdateTypeFilterMerged(
                                        vec![PDB_MAIN_SLOT, PDB_DIFF_SLOT],
                                        String::default(),
                                        false,
                                        false,
                                    ))
                            {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                        }
                    }
                },

                FrontendCommand::ReconstructTypeResult(type_reconstruction_result) => {
                    match type_reconstruction_result {
                        Err(err) => {
                            log::error!("Failed to reconstruct type: {}", err);
                        }
                        Ok(reconstructed_type) => {
                            let last_line_number = 1 + reconstructed_type.lines().count();
                            let line_numbers =
                                (1..last_line_number).fold(String::default(), |mut acc, e| {
                                    let _r = writeln!(&mut acc, "{}", e);
                                    acc
                                });
                            self.current_mode = ResymAppMode::Browsing(
                                line_numbers,
                                last_line_number,
                                reconstructed_type,
                            );
                        }
                    }
                }

                FrontendCommand::DiffTypeResult(type_diff_result) => match type_diff_result {
                    Err(err) => {
                        log::error!("Failed to diff type: {}", err);
                    }
                    Ok(type_diff) => {
                        let mut last_line_number = 1;
                        let (line_numbers_old, line_numbers_new, line_changes) =
                            type_diff.metadata.iter().fold(
                                (String::default(), String::default(), vec![]),
                                |(mut acc_old, mut acc_new, mut acc_changes), metadata| {
                                    let indices = metadata.0;

                                    if let Some(indice) = indices.0 {
                                        last_line_number =
                                            std::cmp::max(last_line_number, 1 + indice);
                                        let _r = writeln!(&mut acc_old, "{}", 1 + indice);
                                    } else {
                                        let _r = writeln!(&mut acc_old);
                                    }

                                    if let Some(indice) = indices.1 {
                                        last_line_number =
                                            std::cmp::max(last_line_number, 1 + indice);
                                        let _r = writeln!(&mut acc_new, "{}", 1 + indice);
                                    } else {
                                        let _r = writeln!(&mut acc_new);
                                    }

                                    acc_changes.push(metadata.1);

                                    (acc_old, acc_new, acc_changes)
                                },
                            );

                        self.current_mode = ResymAppMode::Comparing(
                            line_numbers_old,
                            line_numbers_new,
                            last_line_number,
                            line_changes,
                            type_diff.data,
                        );
                    }
                },

                FrontendCommand::UpdateFilteredTypes(filtered_types) => {
                    self.filtered_type_list = filtered_types;
                    self.selected_row = usize::MAX;
                }
            }
        }
    }

    fn update_menu_bar(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open PDB file").clicked() {
                    if let Some(file_path) = Self::select_pdb_file() {
                        if let Err(err) = self
                            .backend
                            .send_command(BackendCommand::LoadPDB(PDB_MAIN_SLOT, file_path.into()))
                        {
                            log::error!("Failed to load the PDB file: {}", err);
                        }
                    }
                }
                if ui
                    .add_enabled(
                        matches!(self.current_mode, ResymAppMode::Browsing(..)),
                        egui::Button::new("Compare with..."),
                    )
                    .clicked()
                {
                    if let Some(file_path) = Self::select_pdb_file() {
                        if let Err(err) = self
                            .backend
                            .send_command(BackendCommand::LoadPDB(PDB_DIFF_SLOT, file_path.into()))
                        {
                            log::error!("Failed to load the PDB file: {}", err);
                        }
                    }
                }
                if ui.button("Settings").clicked() {
                    self.settings_wnd_open = true;
                }
                if ui.button("Exit").clicked() {
                    frame.quit();
                }
            });
        });
    }

    fn update_type_list(&mut self, ui: &mut egui::Ui) {
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
                            let (type_name, type_index) = &self.filtered_type_list[row_index];

                            if ui
                                .selectable_label(self.selected_row == row_index, type_name)
                                .clicked()
                            {
                                self.selected_row = row_index;
                                match self.current_mode {
                                    ResymAppMode::Browsing(..) => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::ReconstructTypeByIndex(
                                                PDB_MAIN_SLOT,
                                                *type_index,
                                                PrimitiveReconstructionFlavor::Portable,
                                                self.settings.print_header,
                                                self.settings.reconstruct_dependencies,
                                                self.settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type: {}", err);
                                        }
                                    }
                                    ResymAppMode::Comparing(..) => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::DiffTypeByName(
                                                PDB_MAIN_SLOT,
                                                PDB_DIFF_SLOT,
                                                type_name.clone(),
                                                PrimitiveReconstructionFlavor::Portable,
                                                self.settings.print_header,
                                                self.settings.reconstruct_dependencies,
                                                self.settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type diff: {}", err);
                                        }
                                    }
                                    _ => log::error!("Invalid application state"),
                                }
                            }
                        }
                    });
            },
        );
    }

    fn update_console(&mut self, ui: &mut egui::Ui) {
        // Update console
        self.console_content
            .extend(self.logger.read().lines().map(|s| s.to_string()));
        self.logger.clear();

        const TEXT_STYLE: TextStyle = TextStyle::Monospace;
        let row_height = ui.text_style_height(&TEXT_STYLE);
        let num_rows = self.console_content.len();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom()
            .show_rows(ui, row_height, num_rows, |ui, row_range| {
                for row_index in row_range {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.console_content[row_index].as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                }
            });
    }

    fn update_code_view(&mut self, ui: &mut egui::Ui) {
        const LANGUAGE_SYNTAX: &str = "cpp";
        let theme = if self.settings.use_light_theme {
            CodeTheme::light()
        } else {
            CodeTheme::dark()
        };

        let line_desc =
            if let ResymAppMode::Comparing(_, _, _, line_changes, _) = &self.current_mode {
                Some(line_changes)
            } else {
                None
            };

        // Layouter that'll disable wrapping and apply syntax highlighting if needed
        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
            let layout_job = highlight_code(
                ui.ctx(),
                &theme,
                string,
                LANGUAGE_SYNTAX,
                self.settings.enable_syntax_hightlighting,
                line_desc,
            );
            ui.fonts().layout_job(layout_job)
        };

        // Type dump area
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                const LINE_NUMBER_DIGIT_WIDTH: usize = 10;
                let (num_colums, min_column_width) = if self.settings.print_line_numbers {
                    match self.current_mode {
                        ResymAppMode::Comparing(_, _, last_line_number, ..) => {
                            // Compute the columns' sizes from the number of digits
                            let char_count = int_log10(last_line_number);
                            let line_number_width = (char_count * LINE_NUMBER_DIGIT_WIDTH) as f32;

                            // Old index + new index + code editor
                            (3, line_number_width)
                        }
                        ResymAppMode::Browsing(_, last_line_number, _) => {
                            // Compute the columns' sizes from the number of digits
                            let char_count = int_log10(last_line_number);
                            let line_number_width = (char_count * LINE_NUMBER_DIGIT_WIDTH) as f32;

                            // Line numbers + code editor
                            (2, line_number_width)
                        }
                        _ => {
                            // Code editor only
                            (1, 0.0)
                        }
                    }
                } else {
                    // Code editor only
                    (1, 0.0)
                };

                egui::Grid::new("code_editor_grid")
                    .num_columns(num_colums)
                    .min_col_width(min_column_width)
                    .show(ui, |ui| {
                        match &self.current_mode {
                            ResymAppMode::Comparing(
                                line_numbers_old,
                                line_numbers_new,
                                _,
                                _,
                                reconstructed_type_diff,
                            ) => {
                                // Line numbers
                                if self.settings.print_line_numbers {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers_old.as_str())
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers_new.as_str())
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                }
                                // Text content
                                ui.add(
                                    egui::TextEdit::multiline(
                                        &mut reconstructed_type_diff.as_str(),
                                    )
                                    .code_editor()
                                    .layouter(&mut layouter),
                                );
                            }
                            ResymAppMode::Browsing(line_numbers, _, reconstructed_type_content) => {
                                // Line numbers
                                if self.settings.print_line_numbers {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers.as_str())
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                }
                                // Text content
                                ui.add(
                                    egui::TextEdit::multiline(
                                        &mut reconstructed_type_content.as_str(),
                                    )
                                    .code_editor()
                                    .layouter(&mut layouter),
                                );
                            }
                            ResymAppMode::Idle => {}
                        }
                    });
            });
    }

    fn update_settings_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .anchor(egui::Align2::CENTER_CENTER, [0.0; 2])
            .open(&mut self.settings_wnd_open)
            .auto_sized()
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Theme");
                // Show radio-buttons to switch between light and dark mode.
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings.use_light_theme, true, "☀ Light");
                    ui.selectable_value(&mut self.settings.use_light_theme, false, "🌙 Dark");
                });
                ui.add_space(5.0);

                ui.label("Search");
                ui.checkbox(
                    &mut self.settings.search_case_insensitive,
                    "Case insensitive",
                );
                ui.checkbox(
                    &mut self.settings.search_use_regex,
                    "Enable regular expressions",
                );
                ui.add_space(5.0);

                ui.label("Type reconstruction");
                ui.checkbox(
                    &mut self.settings.enable_syntax_hightlighting,
                    "Enable C++ syntax highlighting",
                );
                ui.checkbox(&mut self.settings.print_header, "Print header");
                ui.checkbox(
                    &mut self.settings.reconstruct_dependencies,
                    "Print definitions of referenced types",
                );
                ui.checkbox(
                    &mut self.settings.print_access_specifiers,
                    "Print access specifiers",
                );
                ui.checkbox(&mut self.settings.print_line_numbers, "Print line numbers");
            });
    }

    fn select_pdb_file() -> Option<String> {
        open_file_dialog(
            "Select a PDB file",
            "",
            Some((&["*.pdb"], "PDB files (*.pdb)")),
        )
    }
}

/// This struct represents the persistent settings of the application.
#[derive(Serialize, Deserialize)]
struct ResymAppSettings {
    use_light_theme: bool,
    search_case_insensitive: bool,
    search_use_regex: bool,
    enable_syntax_hightlighting: bool,
    print_header: bool,
    reconstruct_dependencies: bool,
    print_access_specifiers: bool,
    print_line_numbers: bool,
}

impl Default for ResymAppSettings {
    fn default() -> Self {
        Self {
            use_light_theme: false,
            search_case_insensitive: true,
            search_use_regex: false,
            enable_syntax_hightlighting: true,
            print_header: true,
            reconstruct_dependencies: true,
            print_access_specifiers: true,
            print_line_numbers: false,
        }
    }
}

// FIXME: Replace with `checked_log10` once it's stabilized.
fn int_log10<T>(mut i: T) -> usize
where
    T: std::ops::DivAssign + std::cmp::PartialOrd + From<u8> + Copy,
{
    let zero = T::from(0);
    if i == zero {
        return 1;
    }

    let mut len = 0;
    let ten = T::from(10);

    while i > zero {
        i /= ten;
        len += 1;
    }

    len
}
