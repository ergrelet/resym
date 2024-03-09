use anyhow::Result;
use eframe::egui;
use memory_logger::blocking::MemoryLogger;
use resym_core::{
    backend::{Backend, BackendCommand, PDBSlot},
    frontend::FrontendCommand,
};

#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};
use std::{fmt::Write, sync::Arc, vec};

#[cfg(feature = "http")]
use crate::ui_components::OpenURLComponent;
use crate::{
    frontend::EguiFrontendController,
    mode::ResymAppMode,
    module_tree::{ModuleInfo, ModulePath},
    settings::ResymAppSettings,
    ui_components::{
        CodeViewComponent, ConsoleComponent, ModuleTreeComponent, SettingsComponent,
        TextSearchComponent, TypeListComponent,
    },
};

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy)]
pub enum ResymPDBSlots {
    /// Slot for the single PDB or for the PDB we're diffing from
    Main = 0,
    /// Slot used for the PDB we're diffing to
    Diff = 1,
}

impl From<ResymPDBSlots> for PDBSlot {
    fn from(val: ResymPDBSlots) -> Self {
        val as PDBSlot
    }
}

#[derive(PartialEq)]
enum ExplorerTab {
    TypeSearch,
    ModuleBrowsing,
}

/// Struct that represents our GUI application.
/// It contains the whole application's context at all time.
pub struct ResymApp {
    current_mode: ResymAppMode,
    explorer_selected_tab: ExplorerTab,
    type_search: TextSearchComponent,
    type_list: TypeListComponent,
    module_search: TextSearchComponent,
    module_tree: ModuleTreeComponent,
    code_view: CodeViewComponent,
    console: ConsoleComponent,
    settings: SettingsComponent,
    #[cfg(feature = "http")]
    open_url: OpenURLComponent,
    frontend_controller: Arc<EguiFrontendController>,
    backend: Backend,
    /// Field used by wasm32 targets to store PDB file information
    /// temporarily when selecting a PDB file to open.
    #[cfg(target_arch = "wasm32")]
    open_pdb_data: Rc<RefCell<Option<(PDBSlot, String, Vec<u8>)>>>,
}

// GUI-related trait
impl eframe::App for ResymApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Save settings on shutdown
        eframe::set_value(storage, eframe::APP_KEY, &self.settings.app_settings);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // For wasm32 targets, we cannot block in the UI thread so we have to
        // check for PDB file opening results manually in an non-blocking way.
        #[cfg(target_arch = "wasm32")]
        self.process_open_pdb_file_result();

        // Process incoming commands, if any
        self.process_ui_commands();

        // Update theme if needed
        self.process_theme_update(ctx);

        // Update the "Settings" window if open
        self.settings.update(ctx);

        // Update "Open URL" window if open
        #[cfg(feature = "http")]
        self.open_url.update(ctx, &self.backend);

        // Update the top panel (i.e, menu bar)
        self.update_top_panel(ctx);

        // Update the left side panel (i.e., the type search bar and the type list)
        self.update_left_side_panel(ctx);

        // Update the bottom panel (i.e., the console)
        self.update_bottom_panel(ctx);

        // Update the central panel (i.e., the code view)
        self.update_central_panel(ctx);

        // Process drag and drop messages, if any
        self.handle_drag_and_drop(ctx);

        // Request the backend to repaint after a few milliseconds, in case some UI
        // components have been updated, without consuming too much CPU.
        // Note(ergrelet): this is a workaround for the fact that we can't trigger
        // a repaint from another thread for wasm32 targets.
        #[cfg(target_arch = "wasm32")]
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(0.2));
    }
}

// Utility associated functions and methods
impl ResymApp {
    pub fn new(cc: &eframe::CreationContext<'_>, logger: &'static MemoryLogger) -> Result<Self> {
        let (tx_ui, rx_ui) = crossbeam_channel::unbounded::<FrontendCommand>();
        let frontend_controller = Arc::new(EguiFrontendController::new(
            tx_ui,
            rx_ui,
            cc.egui_ctx.clone(),
        ));
        let backend = Backend::new(frontend_controller.clone())?;

        // Load settings on launch
        let app_settings = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            ResymAppSettings::default()
        };

        log::info!("{} {}", PKG_NAME, PKG_VERSION);
        Ok(Self {
            current_mode: ResymAppMode::Idle,
            explorer_selected_tab: ExplorerTab::TypeSearch,
            type_search: TextSearchComponent::new(),
            type_list: TypeListComponent::new(),
            module_search: TextSearchComponent::new(),
            module_tree: ModuleTreeComponent::new(),
            code_view: CodeViewComponent::new(),
            console: ConsoleComponent::new(logger),
            settings: SettingsComponent::new(app_settings),
            #[cfg(feature = "http")]
            open_url: OpenURLComponent::new(),
            frontend_controller,
            backend,
            #[cfg(target_arch = "wasm32")]
            open_pdb_data: Rc::new(RefCell::new(None)),
        })
    }

    fn process_theme_update(&mut self, ctx: &egui::Context) {
        let theme = if self.settings.app_settings.use_light_theme {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        ctx.set_visuals(theme);
    }

    fn update_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // Process keyboard shortcuts, if any
            self.consume_keyboard_shortcuts(ui);

            // The top panel is often a good place for a menu bar
            self.update_menu_bar(ui);
        });
    }

    fn update_left_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("side_panel")
            .default_width(250.0)
            .width_range(100.0..=f32::INFINITY)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.explorer_selected_tab,
                        ExplorerTab::TypeSearch,
                        "Search types",
                    );
                    ui.selectable_value(
                        &mut self.explorer_selected_tab,
                        ExplorerTab::ModuleBrowsing,
                        "Browse modules",
                    );
                });
                ui.separator();

                match self.explorer_selected_tab {
                    ExplorerTab::TypeSearch => {
                        // Callback run when the search query changes
                        let on_query_update = |search_query: &str| {
                            // Update filtered list if filter has changed
                            let result = if let ResymAppMode::Comparing(..) = self.current_mode {
                                self.backend
                                    .send_command(BackendCommand::UpdateTypeFilterMerged(
                                        vec![
                                            ResymPDBSlots::Main as usize,
                                            ResymPDBSlots::Diff as usize,
                                        ],
                                        search_query.to_string(),
                                        self.settings.app_settings.search_case_insensitive,
                                        self.settings.app_settings.search_use_regex,
                                    ))
                            } else {
                                self.backend.send_command(BackendCommand::UpdateTypeFilter(
                                    ResymPDBSlots::Main as usize,
                                    search_query.to_string(),
                                    self.settings.app_settings.search_case_insensitive,
                                    self.settings.app_settings.search_use_regex,
                                ))
                            };
                            if let Err(err) = result {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                        };

                        // Update the type search bar
                        ui.label("Search");
                        self.type_search.update(ui, &on_query_update);
                        ui.separator();
                        ui.add_space(4.0);

                        // Update the type list
                        self.type_list.update(
                            &self.settings.app_settings,
                            &self.current_mode,
                            &self.backend,
                            ui,
                        );
                    }
                    ExplorerTab::ModuleBrowsing => {
                        // Callback run when the search query changes
                        let on_query_update = |search_query: &str| match self.current_mode {
                            ResymAppMode::Browsing(..) | ResymAppMode::Comparing(..) => {
                                // Request a module list update
                                if let Err(err) =
                                    self.backend.send_command(BackendCommand::ListModules(
                                        ResymPDBSlots::Main as usize,
                                        search_query.to_string(),
                                        self.settings.app_settings.search_case_insensitive,
                                        self.settings.app_settings.search_use_regex,
                                    ))
                                {
                                    log::error!("Failed to update module list: {}", err);
                                }
                            }
                            _ => {}
                        };
                        // Update the type search bar
                        ui.label("Search");
                        self.module_search.update(ui, &on_query_update);
                        ui.separator();
                        ui.add_space(4.0);

                        // Callback run when a module is selected in the tree
                        let on_module_selected =
                            |module_path: &ModulePath, module_info: &ModuleInfo| match self
                                .current_mode
                            {
                                ResymAppMode::Browsing(..) => {
                                    if let Err(err) = self.backend.send_command(
                                        BackendCommand::ReconstructModuleByIndex(
                                            ResymPDBSlots::Main as usize,
                                            module_info.pdb_index,
                                            self.settings.app_settings.primitive_types_flavor,
                                            self.settings.app_settings.print_header,
                                        ),
                                    ) {
                                        log::error!("Failed to reconstruct module: {}", err);
                                    }
                                }

                                ResymAppMode::Comparing(..) => {
                                    if let Err(err) =
                                        self.backend.send_command(BackendCommand::DiffModuleByPath(
                                            ResymPDBSlots::Main as usize,
                                            ResymPDBSlots::Diff as usize,
                                            module_path.to_string(),
                                            self.settings.app_settings.primitive_types_flavor,
                                            self.settings.app_settings.print_header,
                                        ))
                                    {
                                        log::error!("Failed to reconstruct type diff: {}", err);
                                    }
                                }

                                _ => log::error!("Invalid application state"),
                            };

                        // Update the module list
                        self.module_tree.update(ctx, ui, &on_module_selected);
                    }
                }
            });
    }

    fn update_bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel")
            .min_height(100.0)
            .resizable(true)
            .show(ctx, |ui| {
                // Console panel
                ui.vertical(|ui| {
                    ui.label("Console");
                    ui.add_space(4.0);

                    // Update the console component
                    self.console.update(ui);
                });
            });
    }

    fn update_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                // The central panel the region left after adding TopPanel's and SidePanel's
                // Put the label on the left
                ui.label(if let ResymAppMode::Comparing(..) = self.current_mode {
                    "Differences between reconstructed type(s) - C++"
                } else {
                    "Reconstructed type(s) - C++"
                });

                // Start displaying buttons from the right
                #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if let ResymAppMode::Browsing(..) = self.current_mode {
                        // Save button handling
                        // Note: not available on wasm32
                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button("💾  Save (Ctrl+S)").clicked() {
                            self.start_save_reconstruted_content();
                        }
                    }
                });
            });
            ui.separator();

            // Update the code view component
            self.code_view
                .update(&self.settings.app_settings, &self.current_mode, ui);
        });
    }

    fn consume_keyboard_shortcuts(&mut self, ui: &mut egui::Ui) {
        /// Keyboard shortcut for opening files
        const CTRL_O_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut {
            modifiers: egui::Modifiers::CTRL,
            logical_key: egui::Key::O,
        };
        ui.input_mut(|input_state| {
            if input_state.consume_shortcut(&CTRL_O_SHORTCUT) {
                self.start_open_pdb_file(ResymPDBSlots::Main as usize);
            }
        });

        /// Keyboard shortcut for opening URLs
        #[cfg(feature = "http")]
        const CTRL_L_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut {
            modifiers: egui::Modifiers::CTRL,
            logical_key: egui::Key::L,
        };
        #[cfg(feature = "http")]
        ui.input_mut(|input_state| {
            if input_state.consume_shortcut(&CTRL_L_SHORTCUT) {
                self.open_url.open(ResymPDBSlots::Main);
            }
        });

        /// Keyboard shortcut for saving reconstructed content
        #[cfg(not(target_arch = "wasm32"))]
        const CTRL_S_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut {
            modifiers: egui::Modifiers::CTRL,
            logical_key: egui::Key::S,
        };
        // Ctrl+S shortcut handling
        // Note: not available on wasm32
        #[cfg(not(target_arch = "wasm32"))]
        ui.input_mut(|input_state| {
            if input_state.consume_shortcut(&CTRL_S_SHORTCUT) {
                self.start_save_reconstruted_content();
            }
        });
    }

    fn process_ui_commands(&mut self) {
        while let Ok(cmd) = self.frontend_controller.rx_ui.try_recv() {
            match cmd {
                FrontendCommand::LoadPDBResult(result) => match result {
                    Err(err) => {
                        log::error!("Failed to load PDB file: {}", err);
                    }
                    Ok(pdb_slot) => {
                        if pdb_slot == ResymPDBSlots::Main as usize {
                            // Unload the PDB used for diffing if one is loaded
                            if let ResymAppMode::Comparing(..) = self.current_mode {
                                if let Err(err) = self.backend.send_command(
                                    BackendCommand::UnloadPDB(ResymPDBSlots::Diff as usize),
                                ) {
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
                                    ResymPDBSlots::Main as usize,
                                    String::default(),
                                    false,
                                    false,
                                ))
                            {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                            // Request a module list update
                            if let Err(err) =
                                self.backend.send_command(BackendCommand::ListModules(
                                    ResymPDBSlots::Main as usize,
                                    String::default(),
                                    false,
                                    false,
                                ))
                            {
                                log::error!("Failed to update module list: {}", err);
                            }
                        } else if pdb_slot == ResymPDBSlots::Diff as usize {
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
                                        vec![
                                            ResymPDBSlots::Main as usize,
                                            ResymPDBSlots::Diff as usize,
                                        ],
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

                FrontendCommand::LoadURLResult(result) => match result {
                    Err(err) => {
                        log::error!("Failed to load URL: {}", err);
                    }
                    Ok((pdb_slot, file_name, data)) => {
                        if let Err(err) = self
                            .backend
                            .send_command(BackendCommand::LoadPDBFromVec(pdb_slot, file_name, data))
                        {
                            log::error!("Failed to load the PDB file: {err}");
                        }
                    }
                },

                FrontendCommand::ReconstructTypeResult(type_reconstruction_result) => {
                    match type_reconstruction_result {
                        Err(err) => {
                            let error_msg = format!("Failed to reconstruct type: {}", err);
                            log::error!("{}", &error_msg);

                            // Show an empty "reconstruted" view
                            self.current_mode =
                                ResymAppMode::Browsing(Default::default(), 0, error_msg);
                        }
                        Ok(reconstructed_type) => {
                            let last_line_number = 1 + reconstructed_type.lines().count();
                            let line_numbers =
                                (1..last_line_number).fold(String::default(), |mut acc, e| {
                                    let _r = writeln!(&mut acc, "{e}");
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

                FrontendCommand::UpdateModuleList(module_list_result) => match module_list_result {
                    Err(err) => {
                        log::error!("Failed to retrieve module list: {}", err);
                    }
                    Ok(module_list) => {
                        self.module_tree.set_module_list(module_list);
                    }
                },

                FrontendCommand::ReconstructModuleResult(module_reconstruction_result) => {
                    match module_reconstruction_result {
                        Err(err) => {
                            let error_msg = format!("Failed to reconstruct module: {}", err);
                            log::error!("{}", &error_msg);

                            // Show an empty "reconstruted" view
                            self.current_mode =
                                ResymAppMode::Browsing(Default::default(), 0, error_msg);
                        }
                        Ok(reconstructed_module) => {
                            let last_line_number = 1 + reconstructed_module.lines().count();
                            let line_numbers =
                                (1..last_line_number).fold(String::default(), |mut acc, e| {
                                    let _r = writeln!(&mut acc, "{e}");
                                    acc
                                });
                            self.current_mode = ResymAppMode::Browsing(
                                line_numbers,
                                last_line_number,
                                reconstructed_module,
                            );
                        }
                    }
                }

                FrontendCommand::DiffResult(type_diff_result) => match type_diff_result {
                    Err(err) => {
                        let error_msg = format!("Failed to generate diff: {}", err);
                        log::error!("{}", &error_msg);

                        // Show an empty "reconstruted" view
                        self.current_mode = ResymAppMode::Comparing(
                            Default::default(),
                            Default::default(),
                            0,
                            vec![],
                            error_msg,
                        );
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
                    self.type_list.update_type_list(filtered_types);
                }
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    fn update_menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open PDB file (Ctrl+O)").clicked() {
                    ui.close_menu();
                    self.start_open_pdb_file(ResymPDBSlots::Main as usize);
                }

                #[cfg(feature = "http")]
                if ui.button("Open URL (Ctrl+L)").clicked() {
                    ui.close_menu();
                    self.open_url.open(ResymPDBSlots::Main);
                }

                // Separate "Open" from "Compare"
                ui.separator();

                if ui
                    .add_enabled(
                        matches!(self.current_mode, ResymAppMode::Browsing(..)),
                        egui::Button::new("Compare with file ..."),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.start_open_pdb_file(ResymPDBSlots::Diff as usize);
                }

                #[cfg(feature = "http")]
                if ui
                    .add_enabled(
                        matches!(self.current_mode, ResymAppMode::Browsing(..)),
                        egui::Button::new("Compare with URL ..."),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    self.open_url.open(ResymPDBSlots::Diff);
                }

                // Separate "Compare" from "Settings"
                ui.separator();

                if ui.button("Settings").clicked() {
                    ui.close_menu();
                    self.settings.open();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Exit").clicked() {
                    ui.close_menu();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    /// Function invoked on `Open PDB File` or when the Ctrl+O shortcut is used
    #[cfg(not(target_arch = "wasm32"))]
    fn start_open_pdb_file(&mut self, pdb_slot: PDBSlot) {
        let file_path_opt = rfd::FileDialog::new()
            .add_filter("PDB files (*.pdb)", &["pdb"])
            .pick_file();
        if let Some(file_path) = file_path_opt {
            if let Err(err) = self
                .backend
                .send_command(BackendCommand::LoadPDBFromPath(pdb_slot, file_path))
            {
                log::error!("Failed to load the PDB file: {err}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start_open_pdb_file(&mut self, pdb_slot: PDBSlot) {
        let open_pdb_data = self.open_pdb_data.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let file_opt = rfd::AsyncFileDialog::new()
                .add_filter("PDB files (*.pdb)", &["pdb"])
                .pick_file()
                .await;
            if let Some(file) = file_opt {
                // We unwrap() the return value to assert that we are not expecting
                // threads to ever fail while holding the lock.
                *open_pdb_data.borrow_mut() = Some((pdb_slot, file.file_name(), file.read().await));
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn process_open_pdb_file_result(&self) {
        // We unwrap() the return value to assert that we are not expecting
        // threads to ever fail while holding the lock.
        if let Some((pdb_slot, pdb_name, pdb_bytes)) = self.open_pdb_data.borrow_mut().take() {
            if let Err(err) = self.backend.send_command(BackendCommand::LoadPDBFromVec(
                pdb_slot, pdb_name, pdb_bytes,
            )) {
                log::error!("Failed to load the PDB file: {err}");
            }
        }
    }

    /// Function invoked on 'Save' or when the Ctrl+S shortcut is used
    #[cfg(not(target_arch = "wasm32"))]
    fn start_save_reconstruted_content(&self) {
        if let ResymAppMode::Browsing(_, _, ref reconstructed_type) = self.current_mode {
            let file_path_opt = rfd::FileDialog::new()
                .add_filter(
                    "C/C++ Source File (*.c;*.cc;*.cpp;*.cxx;*.h;*.hpp;*.hxx)",
                    &["c", "cc", "cpp", "cxx", "h", "hpp", "hxx"],
                )
                .save_file();
            if let Some(file_path) = file_path_opt {
                let write_result = std::fs::write(&file_path, reconstructed_type);
                match write_result {
                    Ok(()) => log::info!(
                        "Reconstructed content has been saved to '{}'.",
                        file_path.display()
                    ),
                    Err(err) => {
                        log::error!("Failed to write reconstructed content to file: {err}");
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_drag_and_drop(&self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Handle dropped files
            if !i.raw.dropped_files.is_empty() {
                // Allow dropping 1 file (to just view it), or 2 files to diff them
                let slots = [ResymPDBSlots::Main as usize, ResymPDBSlots::Diff as usize];
                for (slot, file) in slots.iter().zip(i.raw.dropped_files.iter()) {
                    if let Some(file_path) = &file.path {
                        if let Err(err) = self
                            .backend
                            .send_command(BackendCommand::LoadPDBFromPath(*slot, file_path.into()))
                        {
                            log::error!("Failed to load the PDB file: {err}");
                        }
                    }
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_drag_and_drop(&self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Handle dropped files
            if !i.raw.dropped_files.is_empty() {
                // Allow dropping 1 file (to just view it), or 2 files to diff them
                let slots = [ResymPDBSlots::Main as usize, ResymPDBSlots::Diff as usize];
                for (slot, file) in slots.iter().zip(i.raw.dropped_files.iter()) {
                    if let Some(file_bytes) = file.bytes.clone() {
                        if let Err(err) = self.backend.send_command(
                            BackendCommand::LoadPDBFromArray(*slot, file.name.clone(), file_bytes),
                        ) {
                            log::error!("Failed to load the PDB file: {err}");
                        }
                    }
                }
            }
        });
    }
}
