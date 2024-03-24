use eframe::egui::{self, ScrollArea, TextStyle};
use resym_core::frontend::{TypeIndex, TypeList};

pub struct TypeListComponent {
    filtered_type_list: TypeList,
    selected_row: usize,
    list_ordering: TypeListOrdering,
}

pub enum TypeListOrdering {
    /// Doesn't respect any particular order
    None,
    /// Orders types alphabetically
    Alphabetical,
}

impl TypeListComponent {
    pub fn new(ordering: TypeListOrdering) -> Self {
        Self {
            filtered_type_list: vec![],
            selected_row: usize::MAX,
            list_ordering: ordering,
        }
    }

    pub fn update_type_list(&mut self, type_list: TypeList) {
        self.filtered_type_list = type_list;
        self.selected_row = usize::MAX;

        // Reorder list if needed
        if let TypeListOrdering::Alphabetical = self.list_ordering {
            self.filtered_type_list
                .sort_unstable_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        }
    }

    pub fn update<CB: FnMut(&str, TypeIndex)>(
        &mut self,
        ui: &mut egui::Ui,
        on_type_selected: &mut CB,
    ) {
        let num_rows = self.filtered_type_list.len();
        const TEXT_STYLE: TextStyle = TextStyle::Body;
        let row_height = ui.text_style_height(&TEXT_STYLE);
        ui.with_layout(
            egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                if num_rows == 0 {
                    // Display a default message to make it obvious the list is empty
                    ui.label("No results");
                    return;
                }

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
                                on_type_selected(type_name, *type_index);
                            }
                        }
                    });
            },
        );
    }
}

impl Default for TypeListComponent {
    fn default() -> Self {
        Self::new(TypeListOrdering::None)
    }
}
