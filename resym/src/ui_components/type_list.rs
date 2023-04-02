use eframe::egui::{self, ScrollArea, TextStyle};
use resym_core::{
    backend::{Backend, BackendCommand},
    frontend::TypeList,
};

use crate::{mode::ResymAppMode, resym_app::ResymPDBSlots, settings::ResymAppSettings};

pub struct TypeListComponent {
    filtered_type_list: TypeList,
    selected_row: usize,
}

impl TypeListComponent {
    pub fn new() -> Self {
        Self {
            filtered_type_list: vec![],
            selected_row: usize::MAX,
        }
    }

    pub fn update_type_list(&mut self, type_list: TypeList) {
        self.filtered_type_list = type_list;
        self.selected_row = usize::MAX;
    }

    pub fn update(
        &mut self,
        app_settings: &ResymAppSettings,
        current_mode: &ResymAppMode,
        backend: &Backend,
        ui: &mut egui::Ui,
    ) {
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
                                match current_mode {
                                    ResymAppMode::Browsing(..) => {
                                        if let Err(err) = backend.send_command(
                                            BackendCommand::ReconstructTypeByIndex(
                                                ResymPDBSlots::Main as usize,
                                                *type_index,
                                                app_settings.primitive_types_flavor,
                                                app_settings.print_header,
                                                app_settings.reconstruct_dependencies,
                                                app_settings.print_access_specifiers,
                                            ),
                                        ) {
                                            log::error!("Failed to reconstruct type: {}", err);
                                        }
                                    }
                                    ResymAppMode::Comparing(..) => {
                                        if let Err(err) =
                                            backend.send_command(BackendCommand::DiffTypeByName(
                                                ResymPDBSlots::Main as usize,
                                                ResymPDBSlots::Diff as usize,
                                                type_name.clone(),
                                                app_settings.primitive_types_flavor,
                                                app_settings.print_header,
                                                app_settings.reconstruct_dependencies,
                                                app_settings.print_access_specifiers,
                                            ))
                                        {
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
}
