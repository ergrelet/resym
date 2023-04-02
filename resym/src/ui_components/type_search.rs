use eframe::egui;
use resym_core::backend::{Backend, BackendCommand};

use crate::{mode::ResymAppMode, resym_app::ResymPDBSlots, settings::ResymAppSettings};

pub struct TypeSearchComponent {
    search_filter: String,
}

impl TypeSearchComponent {
    pub fn new() -> Self {
        Self {
            search_filter: String::default(),
        }
    }

    pub fn update(
        &mut self,
        app_settings: &ResymAppSettings,
        current_mode: &ResymAppMode,
        backend: &Backend,
        ui: &mut egui::Ui,
    ) {
        if ui.text_edit_singleline(&mut self.search_filter).changed() {
            // Update filtered list if filter has changed
            let result = if let ResymAppMode::Comparing(..) = current_mode {
                backend.send_command(BackendCommand::UpdateTypeFilterMerged(
                    vec![ResymPDBSlots::Main as usize, ResymPDBSlots::Diff as usize],
                    self.search_filter.clone(),
                    app_settings.search_case_insensitive,
                    app_settings.search_use_regex,
                ))
            } else {
                backend.send_command(BackendCommand::UpdateTypeFilter(
                    ResymPDBSlots::Main as usize,
                    self.search_filter.clone(),
                    app_settings.search_case_insensitive,
                    app_settings.search_use_regex,
                ))
            };
            if let Err(err) = result {
                log::error!("Failed to update type filter value: {}", err);
            }
        }
    }
}
