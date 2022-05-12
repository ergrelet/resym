#![windows_subsystem = "windows"]

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};
use eframe::{
    egui::{self, ScrollArea, TextStyle},
    epaint::text::LayoutJob,
    epi,
};
use memory_logger::blocking::MemoryLogger;
use serde::{Deserialize, Serialize};
use syntect::{easy::HighlightLines, highlighting::FontStyle, util::LinesWithEndings};
use tinyfiledialogs::open_file_dialog;

use std::sync::{Arc, RwLock};

use resym_core::{
    backend::{Backend, BackendCommand, PDBSlot},
    frontend::{FrontendCommand, FrontendController},
    syntax_highlighting::{self, CodeTheme},
};

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Slot for the single PDB or for the PDB we're diffing from
const PDB_MAIN_SLOT: PDBSlot = 0;
/// Slot used for the PDB we're diffing to
const PDB_DIFF_SLOT: PDBSlot = 1;

fn main() -> Result<()> {
    let logger = MemoryLogger::setup(log::Level::Info)?;
    let app = ResymApp::new(logger)?;
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app), native_options);
}

/// Struct that represents our GUI application.
/// It contains the whole application's context at all time.
struct ResymApp {
    logger: &'static MemoryLogger,
    filtered_type_list: Vec<(String, pdb::TypeIndex)>,
    selected_row: usize,
    search_filter: String,
    reconstructed_type_content: String,
    console_content: Vec<String>,
    settings_wnd_open: bool,
    settings: ResymAppSettings,
    current_mode: ResymAppMode,
    frontend_controller: Arc<EguiFrontendController>,
    backend: Backend,
}

// GUI-related trait
impl epi::App for ResymApp {
    fn name(&self) -> &str {
        PKG_NAME
    }

    /// Called once before the first frame.
    fn setup(
        &mut self,
        _ctx: &egui::Context,
        frame: &epi::Frame,
        storage: Option<&dyn epi::Storage>,
    ) {
        log::info!("{} {}", PKG_NAME, PKG_VERSION);
        // If this fails, let it burn
        self.frontend_controller
            .set_ui_frame(frame.clone())
            .unwrap();

        // Load settings on launch
        if let Some(storage) = storage {
            self.settings = epi::get_value(storage, epi::APP_KEY).unwrap_or_default()
        }
    }

    fn save(&mut self, storage: &mut dyn epi::Storage) {
        // Save settings on shutdown
        epi::set_value(storage, epi::APP_KEY, &self.settings);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
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
                    let result = if self.current_mode == ResymAppMode::Comparing {
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
            ui.label(if self.current_mode == ResymAppMode::Comparing {
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
    fn new(logger: &'static MemoryLogger) -> Result<Self> {
        let (tx_ui, rx_ui) = crossbeam_channel::unbounded::<FrontendCommand>();
        let frontend_controller = Arc::new(EguiFrontendController::new(tx_ui, rx_ui));
        let backend = Backend::new(frontend_controller.clone())?;

        Ok(Self {
            logger,
            filtered_type_list: vec![],
            selected_row: usize::MAX,
            search_filter: String::default(),
            reconstructed_type_content: String::default(),
            console_content: vec![],
            settings_wnd_open: false,
            settings: ResymAppSettings::default(),
            current_mode: ResymAppMode::Idle,
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
                            if self.current_mode == ResymAppMode::Comparing {
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

                            self.current_mode = ResymAppMode::Browsing;
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
                            self.current_mode = ResymAppMode::Comparing;
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

                FrontendCommand::UpdateReconstructedType(data) => {
                    self.reconstructed_type_content = data;
                }

                FrontendCommand::UpdateFilteredTypes(filtered_types) => {
                    self.filtered_type_list = filtered_types;
                    self.selected_row = usize::MAX;
                }
            }
        }
    }

    fn update_menu_bar(&mut self, ui: &mut egui::Ui, frame: &epi::Frame) {
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
                        self.current_mode == ResymAppMode::Browsing,
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
                                    ResymAppMode::Browsing => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::ReconstructTypeByIndex(
                                                PDB_MAIN_SLOT,
                                                *type_index,
                                                self.settings.print_header,
                                                self.settings.reconstruct_dependencies,
                                                self.settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type: {}", err);
                                        }
                                    }
                                    ResymAppMode::Comparing => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::DiffTypeByName(
                                                PDB_MAIN_SLOT,
                                                PDB_DIFF_SLOT,
                                                type_name.clone(),
                                                self.settings.print_header,
                                                self.settings.reconstruct_dependencies,
                                                self.settings.print_access_specifiers,
                                                self.settings.print_line_numbers,
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
            syntax_highlighting::CodeTheme::light()
        } else {
            syntax_highlighting::CodeTheme::dark()
        };

        // Layouter that'll apply syntax highlighting
        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job = highlight_code(ui.ctx(), &theme, string, LANGUAGE_SYNTAX);
            layout_job.wrap_width = wrap_width;
            ui.fonts().layout_job(layout_job)
        };

        // Type dump area
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut text_edit_content = self.reconstructed_type_content.as_str();
                let mut text_edit = egui::TextEdit::multiline(&mut text_edit_content)
                    .code_editor()
                    .desired_width(f32::INFINITY);
                // Override layouter only if needed
                if self.settings.enable_syntax_hightlighting {
                    text_edit = text_edit.layouter(&mut layouter);
                }
                ui.add(text_edit);
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
                ui.add_space(5.0);

                ui.label("Diffing");
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

#[derive(PartialEq)]
enum ResymAppMode {
    Idle,
    Browsing,
    Comparing,
}

/// This struct enables the backend to communicate with us (the frontend)
struct EguiFrontendController {
    tx_ui: Sender<FrontendCommand>,
    rx_ui: Receiver<FrontendCommand>,
    ui_frame: RwLock<Option<epi::Frame>>,
}

impl FrontendController for EguiFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        self.tx_ui.send(command)?;
        // Force the UI backend to call our app's update function on the other end
        if let Ok(ui_frame_opt) = self.ui_frame.try_read() {
            if let Some(ui_frame) = ui_frame_opt.as_ref() {
                ui_frame.request_repaint();
            }
        }
        Ok(())
    }
}

impl EguiFrontendController {
    pub fn new(tx_ui: Sender<FrontendCommand>, rx_ui: Receiver<FrontendCommand>) -> Self {
        Self {
            tx_ui,
            rx_ui,
            ui_frame: RwLock::new(None),
        }
    }

    fn set_ui_frame(&self, ui_frame: epi::Frame) -> Result<()> {
        match self.ui_frame.write() {
            Err(_) => Err(anyhow!("Failed to update `ui_frame`")),
            Ok(mut ui_frame_opt) => {
                *ui_frame_opt = Some(ui_frame);
                Ok(())
            }
        }
    }
}

/// Memoized code highlighting
fn highlight_code(ctx: &egui::Context, theme: &CodeTheme, code: &str, language: &str) -> LayoutJob {
    impl egui::util::cache::ComputerMut<(&CodeTheme, &str, &str), LayoutJob> for CodeHighlighter {
        fn compute(&mut self, (theme, code, lang): (&CodeTheme, &str, &str)) -> LayoutJob {
            self.highlight(theme, code, lang)
        }
    }

    type HighlightCache<'a> = egui::util::cache::FrameCache<LayoutJob, CodeHighlighter>;

    let mut memory = ctx.memory();
    let highlight_cache = memory.caches.cache::<HighlightCache<'_>>();
    highlight_cache.get((theme, code, language))
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
    fn highlight(&self, theme: &CodeTheme, code: &str, lang: &str) -> LayoutJob {
        self.highlight_impl(theme, code, lang).unwrap_or_else(|| {
            // Fallback:
            LayoutJob::simple(
                code.into(),
                egui::FontId::monospace(14.0),
                if theme.dark_mode {
                    egui::Color32::LIGHT_GRAY
                } else {
                    egui::Color32::DARK_GRAY
                },
                f32::INFINITY,
            )
        })
    }

    fn highlight_impl(&self, theme: &CodeTheme, text: &str, language: &str) -> Option<LayoutJob> {
        let syntax = self
            .ps
            .find_syntax_by_name(language)
            .or_else(|| self.ps.find_syntax_by_extension(language))?;

        let theme = theme.syntect_theme.syntect_key_name();
        let mut h = HighlightLines::new(syntax, &self.ts.themes[theme]);

        use egui::text::{LayoutSection, TextFormat};

        let mut job = LayoutJob {
            text: text.into(),
            ..Default::default()
        };

        for line in LinesWithEndings::from(text) {
            let mut bg_color = egui::Color32::TRANSPARENT;
            for (style, range) in h.highlight(line, &self.ps) {
                // Change the background of regions that have been affected in the diff.
                // FIXME: This is really dirty, do better.
                if range == "+" {
                    bg_color = egui::Color32::DARK_GREEN;
                } else if range == "-" {
                    bg_color = egui::Color32::DARK_RED;
                } else if range == "\n" {
                    bg_color = egui::Color32::TRANSPARENT;
                }

                let fg = style.foreground;
                let text_color = egui::Color32::from_rgb(fg.r, fg.g, fg.b);
                let italics = style.font_style.contains(FontStyle::ITALIC);
                let underline = style.font_style.contains(FontStyle::ITALIC);
                let underline = if underline {
                    egui::Stroke::new(1.0, text_color)
                } else {
                    egui::Stroke::none()
                };
                job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: as_byte_range(text, range),
                    format: TextFormat {
                        background: bg_color,
                        font_id: egui::FontId::monospace(14.0),
                        color: text_color,
                        italics,
                        underline,
                        ..Default::default()
                    },
                });
            }
        }

        Some(job)
    }
}

fn as_byte_range(whole: &str, range: &str) -> std::ops::Range<usize> {
    let whole_start = whole.as_ptr() as usize;
    let range_start = range.as_ptr() as usize;
    assert!(whole_start <= range_start);
    assert!(range_start + range.len() <= whole_start + whole.len());
    let offset = range_start - whole_start;
    offset..(offset + range.len())
}
