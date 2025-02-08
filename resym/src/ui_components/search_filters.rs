use bevy_reflect::{Reflect, Struct};
use eframe::egui;

pub struct SearchFiltersComponent<FilterType> {
    header: String,
    filters: FilterType,
}

impl<FilterType: Default + Clone + Reflect + Struct> SearchFiltersComponent<FilterType> {
    pub fn new(header: &str) -> Self {
        Self {
            header: header.into(),
            filters: FilterType::default(),
        }
    }

    pub fn filters(&self) -> &FilterType {
        &self.filters
    }

    /// Update/render the UI component
    pub fn update<CB: Fn(&FilterType)>(&mut self, ui: &mut egui::Ui, on_filter_update: &CB) {
        ui.collapsing(&self.header, |ui| {
            ui.horizontal(|ui| {
                let field_count = self.filters.field_len();
                // Iterate over the struct's fields
                for i in 0..field_count {
                    // Get current field name as a string
                    let field_name = self
                        .filters
                        .name_at(i)
                        .expect("name_at should succeed")
                        .to_string();
                    // Get mutable ref (as bool) to the current field
                    let field_value = self
                        .filters
                        .field_at_mut(i)
                        .expect("field_at_mut should succeed")
                        .try_downcast_mut::<bool>()
                        .expect("filter field should be bool");

                    if ui.checkbox(field_value, field_name).changed() {
                        on_filter_update(&self.filters);
                    }
                }
            });
        });
    }
}
