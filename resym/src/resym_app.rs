use anyhow::Result;
use eframe::egui;
use memory_logger::blocking::MemoryLogger;
use resym_core::{
    backend::{Backend, BackendCommand, PDBSlot, SymbolFilters},
    frontend::FrontendCommand,
    pdb_file::{SymbolIndex, TypeIndex},
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
        CodeViewComponent, ConsoleComponent, IndexListComponent, IndexListOrdering,
        ModuleTreeComponent, SearchFiltersComponent, SettingsComponent, TextSearchComponent,
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

/// Tabs available for the left-side panel
#[derive(PartialEq)]
enum LeftPanelTab {
    TypeSearch,
    SymbolSearch,
    ModuleBrowsing,
}

/// Tabs available for the bottom panel
#[derive(PartialEq)]
enum BottomPanelTab {
    Console,
    XRefsTo,
    XRefsFrom,
}

/// Struct that represents our GUI application.
/// It contains the whole application's context at all time.
pub struct ResymApp {
    current_mode: ResymAppMode,
    // Components used in the left-side panel
    left_panel_selected_tab: LeftPanelTab,
    type_search: TextSearchComponent,
    type_list: IndexListComponent<TypeIndex>,
    selected_type_index: Option<TypeIndex>,
    symbol_search: TextSearchComponent,
    symbol_filters: SearchFiltersComponent<SymbolFilters>,
    symbol_list: IndexListComponent<SymbolIndex>,
    selected_symbol_index: Option<SymbolIndex>,
    module_search: TextSearchComponent,
    module_tree: ModuleTreeComponent,
    code_view: CodeViewComponent,
    // Components used in the bottom panel
    bottom_panel_selected_tab: BottomPanelTab,
    console: ConsoleComponent,
    xref_to_list: IndexListComponent<TypeIndex>,
    xref_from_list: IndexListComponent<TypeIndex>,
    // Other components
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
            left_panel_selected_tab: LeftPanelTab::TypeSearch,
            type_search: TextSearchComponent::new(),
            type_list: IndexListComponent::new(IndexListOrdering::Alphabetical),
            selected_type_index: None,
            symbol_search: TextSearchComponent::new(),
            symbol_filters: SearchFiltersComponent::new("Search filters"),
            symbol_list: IndexListComponent::new(IndexListOrdering::Alphabetical),
            selected_symbol_index: None,
            module_search: TextSearchComponent::new(),
            module_tree: ModuleTreeComponent::new(),
            code_view: CodeViewComponent::new(),
            bottom_panel_selected_tab: BottomPanelTab::Console,
            console: ConsoleComponent::new(logger),
            xref_to_list: IndexListComponent::new(IndexListOrdering::Alphabetical),
            xref_from_list: IndexListComponent::new(IndexListOrdering::Alphabetical),
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
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.left_panel_selected_tab,
                        LeftPanelTab::TypeSearch,
                        "Search types",
                    );
                    ui.selectable_value(
                        &mut self.left_panel_selected_tab,
                        LeftPanelTab::SymbolSearch,
                        "Search symbols",
                    );
                    ui.selectable_value(
                        &mut self.left_panel_selected_tab,
                        LeftPanelTab::ModuleBrowsing,
                        "Browse modules",
                    );
                });
                ui.separator();

                match self.left_panel_selected_tab {
                    LeftPanelTab::TypeSearch => {
                        // Callback run when the search query changes
                        let on_query_update = |search_query: &str| {
                            // Update filtered list if filter has changed
                            let result = if let ResymAppMode::Comparing(..) = self.current_mode {
                                self.backend.send_command(BackendCommand::ListTypesMerged(
                                    vec![
                                        ResymPDBSlots::Main as usize,
                                        ResymPDBSlots::Diff as usize,
                                    ],
                                    search_query.to_string(),
                                    self.settings.app_settings.search_case_insensitive,
                                    self.settings.app_settings.search_use_regex,
                                    self.settings.app_settings.ignore_std_types,
                                ))
                            } else {
                                self.backend.send_command(BackendCommand::ListTypes(
                                    ResymPDBSlots::Main as usize,
                                    search_query.to_string(),
                                    self.settings.app_settings.search_case_insensitive,
                                    self.settings.app_settings.search_use_regex,
                                    self.settings.app_settings.ignore_std_types,
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

                        // Callback run when a type is selected in the list
                        let mut on_type_selected = |type_name: &str, type_index: TypeIndex| {
                            // Update currently selected type index
                            self.selected_type_index = Some(type_index);

                            match self.current_mode {
                                ResymAppMode::Browsing(..) => {
                                    if let Err(err) = self.backend.send_command(
                                        BackendCommand::ReconstructTypeByIndex(
                                            ResymPDBSlots::Main as usize,
                                            type_index,
                                            self.settings.app_settings.primitive_types_flavor,
                                            self.settings.app_settings.print_header,
                                            self.settings.app_settings.reconstruct_dependencies,
                                            self.settings.app_settings.print_access_specifiers,
                                            self.settings.app_settings.integers_as_hexadecimal,
                                            self.settings.app_settings.ignore_std_types,
                                        ),
                                    ) {
                                        log::error!("Failed to reconstruct type: {}", err);
                                    }
                                }
                                ResymAppMode::Comparing(..) => {
                                    if let Err(err) =
                                        self.backend.send_command(BackendCommand::DiffTypeByName(
                                            ResymPDBSlots::Main as usize,
                                            ResymPDBSlots::Diff as usize,
                                            type_name.to_string(),
                                            self.settings.app_settings.primitive_types_flavor,
                                            self.settings.app_settings.print_header,
                                            self.settings.app_settings.reconstruct_dependencies,
                                            self.settings.app_settings.print_access_specifiers,
                                            self.settings.app_settings.integers_as_hexadecimal,
                                            self.settings.app_settings.ignore_std_types,
                                        ))
                                    {
                                        log::error!("Failed to reconstruct type diff: {}", err);
                                    }
                                }
                                _ => log::error!("Invalid application state"),
                            }
                        };
                        // Update the type list
                        self.type_list.update(ui, &mut on_type_selected);
                    }

                    LeftPanelTab::SymbolSearch => {
                        let update_symbol_list =
                            |search_query: &str, search_filters: &SymbolFilters| {
                                // Update filtered list if filter has changed
                                let result = if let ResymAppMode::Comparing(..) = self.current_mode
                                {
                                    self.backend.send_command(BackendCommand::ListSymbolsMerged(
                                        vec![
                                            ResymPDBSlots::Main as usize,
                                            ResymPDBSlots::Diff as usize,
                                        ],
                                        search_query.to_string(),
                                        self.settings.app_settings.search_case_insensitive,
                                        self.settings.app_settings.search_use_regex,
                                        self.settings.app_settings.ignore_std_types,
                                        search_filters.clone(),
                                    ))
                                } else {
                                    self.backend.send_command(BackendCommand::ListSymbols(
                                        ResymPDBSlots::Main as usize,
                                        search_query.to_string(),
                                        self.settings.app_settings.search_case_insensitive,
                                        self.settings.app_settings.search_use_regex,
                                        self.settings.app_settings.ignore_std_types,
                                        search_filters.clone(),
                                    ))
                                };
                                if let Err(err) = result {
                                    log::error!("Failed to update type filter value: {}", err);
                                }
                            };

                        // Callback run when the search query is updated
                        let on_query_update = |search_query: &str| {
                            let search_filters = self.symbol_filters.filters();
                            update_symbol_list(search_query, search_filters);
                        };

                        // Update the symbol search bar
                        ui.label("Search");
                        self.symbol_search.update(ui, &on_query_update);

                        // Callback run when the search filter is updated
                        let on_filter_update = |search_filters: &SymbolFilters| {
                            let search_query = self.symbol_search.search_filter();
                            update_symbol_list(search_query, search_filters);
                        };
                        self.symbol_filters.update(ui, &on_filter_update);
                        ui.separator();
                        ui.add_space(4.0);

                        // Callback run when a type is selected in the list
                        let mut on_symbol_selected =
                            |symbol_name: &str, symbol_index: SymbolIndex| {
                                // Update currently selected type index
                                self.selected_symbol_index = Some(symbol_index);

                                match self.current_mode {
                                    ResymAppMode::Browsing(..) => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::ReconstructSymbolByIndex(
                                                ResymPDBSlots::Main as usize,
                                                symbol_index,
                                                self.settings.app_settings.primitive_types_flavor,
                                                self.settings.app_settings.print_header,
                                                self.settings.app_settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type: {}", err);
                                        }
                                    }
                                    ResymAppMode::Comparing(..) => {
                                        if let Err(err) = self.backend.send_command(
                                            BackendCommand::DiffSymbolByName(
                                                ResymPDBSlots::Main as usize,
                                                ResymPDBSlots::Diff as usize,
                                                symbol_name.to_string(),
                                                self.settings.app_settings.primitive_types_flavor,
                                                self.settings.app_settings.print_header,
                                                self.settings.app_settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type diff: {}", err);
                                        }
                                    }
                                    _ => log::error!("Invalid application state"),
                                }
                            };

                        // Update the symbol list
                        self.symbol_list.update(ui, &mut on_symbol_selected);
                    }

                    LeftPanelTab::ModuleBrowsing => {
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
                                            self.settings.app_settings.print_access_specifiers,
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
                                            self.settings.app_settings.print_access_specifiers,
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

    /// Update/render the bottom panel component and its sub-components
    fn update_bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel")
            .min_height(100.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    // Tab headers
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.bottom_panel_selected_tab,
                            BottomPanelTab::Console,
                            "Console",
                        );

                        if let ResymAppMode::Browsing(..) = self.current_mode {
                            // Only display those tabs in browsing mode
                            ui.selectable_value(
                                &mut self.bottom_panel_selected_tab,
                                BottomPanelTab::XRefsTo,
                                "XRefs to",
                            );
                            ui.selectable_value(
                                &mut self.bottom_panel_selected_tab,
                                BottomPanelTab::XRefsFrom,
                                "XRefs from",
                            );
                        }
                    });
                    ui.separator();

                    let mut on_type_selected = |_: &str, type_index: TypeIndex| {
                        // Update currently selected type index
                        self.selected_type_index = Some(type_index);

                        // Note: only support "Browsing" mode
                        if let ResymAppMode::Browsing(..) = self.current_mode {
                            if let Err(err) =
                                self.backend
                                    .send_command(BackendCommand::ReconstructTypeByIndex(
                                        ResymPDBSlots::Main as usize,
                                        type_index,
                                        self.settings.app_settings.primitive_types_flavor,
                                        self.settings.app_settings.print_header,
                                        self.settings.app_settings.reconstruct_dependencies,
                                        self.settings.app_settings.print_access_specifiers,
                                        self.settings.app_settings.integers_as_hexadecimal,
                                        self.settings.app_settings.ignore_std_types,
                                    ))
                            {
                                log::error!("Failed to reconstruct type: {}", err);
                            }
                        }
                    };

                    // Tab body
                    match self.bottom_panel_selected_tab {
                        BottomPanelTab::Console => {
                            // Console panel
                            self.console.update(ui);
                        }
                        BottomPanelTab::XRefsTo => {
                            // Update xref list
                            self.xref_to_list.update(ui, &mut on_type_selected);
                        }
                        BottomPanelTab::XRefsFrom => {
                            // Update xref list
                            self.xref_from_list.update(ui, &mut on_type_selected);
                        }
                    }
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
                    // Fetures only available in "Browsing" mode
                    if let ResymAppMode::Browsing(..) = self.current_mode {
                        // Save button
                        // Note: not available on wasm32
                        #[cfg(not(target_arch = "wasm32"))]
                        if ui.button("💾  Save (Ctrl+S)").clicked() {
                            self.start_save_reconstruted_content();
                        }

                        // Cross-references button
                        if let Some(selected_type_index) = self.selected_type_index {
                            if ui.button("🔍  Find XRefs to (Alt+X)").clicked() {
                                self.list_xrefs_for_type(selected_type_index);
                            }
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

        // Keyboard shortcut for opening URLs
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

        // Keyboard shortcut for finding cross-references
        const ALT_X_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut {
            modifiers: egui::Modifiers::ALT,
            logical_key: egui::Key::X,
        };
        ui.input_mut(|input_state| {
            if input_state.consume_shortcut(&ALT_X_SHORTCUT) {
                if let Some(selected_type_index) = self.selected_type_index {
                    self.list_xrefs_for_type(selected_type_index);
                }
            }
        });

        // Keyboard shortcut for saving reconstructed content
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

                            // Reset current mode
                            self.current_mode =
                                ResymAppMode::Browsing(String::default(), 0, String::default());
                            // Reset selected type
                            self.selected_type_index = None;
                            // Reset xref lists
                            self.xref_to_list.update_index_list(vec![]);
                            self.xref_from_list.update_index_list(vec![]);

                            // Request a type list update
                            if let Err(err) = self.backend.send_command(BackendCommand::ListTypes(
                                ResymPDBSlots::Main as usize,
                                Default::default(),
                                false,
                                false,
                                self.settings.app_settings.ignore_std_types,
                            )) {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                            // Request a symbol list update
                            if let Err(err) =
                                self.backend.send_command(BackendCommand::ListSymbols(
                                    ResymPDBSlots::Main as usize,
                                    Default::default(),
                                    false,
                                    false,
                                    self.settings.app_settings.ignore_std_types,
                                    Default::default(),
                                ))
                            {
                                log::error!("Failed to update type filter value: {}", err);
                            }
                            // Request a module list update
                            if let Err(err) =
                                self.backend.send_command(BackendCommand::ListModules(
                                    ResymPDBSlots::Main as usize,
                                    Default::default(),
                                    false,
                                    false,
                                ))
                            {
                                log::error!("Failed to update module list: {}", err);
                            }
                        } else if pdb_slot == ResymPDBSlots::Diff as usize {
                            // Reset current mode
                            self.current_mode = ResymAppMode::Comparing(
                                Default::default(),
                                Default::default(),
                                0,
                                Default::default(),
                                Default::default(),
                            );
                            // Reset selected type
                            self.selected_type_index = None;
                            // Reset xref lists
                            self.xref_to_list.update_index_list(vec![]);
                            self.xref_from_list.update_index_list(vec![]);

                            // Request a type list update
                            if let Err(err) =
                                self.backend.send_command(BackendCommand::ListTypesMerged(
                                    vec![
                                        ResymPDBSlots::Main as usize,
                                        ResymPDBSlots::Diff as usize,
                                    ],
                                    Default::default(),
                                    false,
                                    false,
                                    self.settings.app_settings.ignore_std_types,
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
                        Ok((reconstructed_type, xrefs_from)) => {
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

                            // Update xref lists
                            self.xref_to_list.update_index_list(vec![]);
                            self.xref_from_list.update_index_list(xrefs_from);
                            // Switch to the "xref from" tab
                            self.bottom_panel_selected_tab = BottomPanelTab::XRefsFrom;
                        }
                    }
                }

                FrontendCommand::ListModulesResult(module_list_result) => {
                    match module_list_result {
                        Err(err) => {
                            log::error!("Failed to retrieve module list: {}", err);
                        }
                        Ok(module_list) => {
                            self.module_tree.set_module_list(module_list);
                        }
                    }
                }

                FrontendCommand::ReconstructSymbolResult(result) => {
                    match result {
                        Err(err) => {
                            let error_msg = format!("Failed to reconstruct symbol: {}", err);
                            log::error!("{}", &error_msg);

                            // Show an empty "reconstruted" view
                            self.current_mode =
                                ResymAppMode::Browsing(Default::default(), 0, error_msg);
                        }
                        Ok(reconstructed_symbol) => {
                            let last_line_number = 1 + reconstructed_symbol.lines().count();
                            let line_numbers =
                                (1..last_line_number).fold(String::default(), |mut acc, e| {
                                    let _r = writeln!(&mut acc, "{e}");
                                    acc
                                });
                            self.current_mode = ResymAppMode::Browsing(
                                line_numbers,
                                last_line_number,
                                reconstructed_symbol,
                            );
                        }
                    }
                }

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

                FrontendCommand::ListTypesResult(filtered_types) => {
                    // Update type list component
                    self.type_list.update_index_list(filtered_types);
                }

                FrontendCommand::ListSymbolsResult(filtered_symbols) => {
                    // Update symbol list component
                    self.symbol_list.update_index_list(filtered_symbols);
                }

                FrontendCommand::ListTypeCrossReferencesResult(xref_list_result) => {
                    match xref_list_result {
                        Err(err) => {
                            log::error!("Failed to list cross-references: {err}");
                        }
                        Ok(xref_list) => {
                            let xref_count = xref_list.len();
                            log::info!("{xref_count} cross-references found!");

                            // Update xref list component
                            self.xref_to_list.update_index_list(xref_list);
                            // Switch to xref tab
                            self.bottom_panel_selected_tab = BottomPanelTab::XRefsTo;
                        }
                    }
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
        let file_path_opt = tinyfiledialogs::open_file_dialog(
            "Select a PDB file",
            "",
            Some((&["*.pdb"], "PDB files (*.pdb)")),
        );
        if let Some(file_path) = file_path_opt {
            if let Err(err) = self
                .backend
                .send_command(BackendCommand::LoadPDBFromPath(pdb_slot, file_path.into()))
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

    /// Function invoked on 'Find XRefs to'
    fn list_xrefs_for_type(&self, type_index: TypeIndex) {
        log::info!(
            "Looking for cross-references for type #0x{:x}...",
            type_index
        );
        if let Err(err) = self
            .backend
            .send_command(BackendCommand::ListTypeCrossReferences(
                ResymPDBSlots::Main as usize,
                type_index,
            ))
        {
            log::error!(
                "Failed to list cross-references to type #0x{:x}: {err}",
                type_index
            );
        }
    }

    /// Function invoked on 'Save' or when the Ctrl+S shortcut is used
    #[cfg(not(target_arch = "wasm32"))]
    fn start_save_reconstruted_content(&self) {
        if let ResymAppMode::Browsing(_, _, ref reconstructed_type) = self.current_mode {
            let file_path_opt = tinyfiledialogs::save_file_dialog_with_filter(
                "Save content to file",
                "",
                &["*.c", "*.cc", "*.cpp", "*.cxx", "*.h", "*.hpp", "*.hxx"],
                "C/C++ Source File (*.c;*.cc;*.cpp;*.cxx;*.h;*.hpp;*.hxx)",
            );
            if let Some(file_path) = file_path_opt {
                let write_result = std::fs::write(&file_path, reconstructed_type);
                match write_result {
                    Ok(()) => log::info!("Reconstructed content has been saved to '{file_path}'."),
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
