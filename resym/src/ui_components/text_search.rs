use eframe::egui;

pub struct TextSearchComponent {
    search_filter: String,
}

impl TextSearchComponent {
    pub fn new() -> Self {
        Self {
            search_filter: String::default(),
        }
    }

    pub fn search_filter(&self) -> &str {
        self.search_filter.as_str()
    }

    /// Update/render the UI component
    pub fn update<CB: Fn(&str)>(&mut self, ui: &mut egui::Ui, on_query_update: &CB) {
        if ui.text_edit_singleline(&mut self.search_filter).changed() {
            on_query_update(self.search_filter());
        }
    }
}
