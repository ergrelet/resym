use eframe::egui::{self, ScrollArea, TextStyle};

pub struct IndexListComponent<I: Copy> {
    index_list: Vec<(String, I)>,
    selected_row: usize,
    list_ordering: IndexListOrdering,
}

pub enum IndexListOrdering {
    /// Doesn't respect any particular order
    None,
    /// Orders types alphabetically
    Alphabetical,
}

impl<I: Copy> IndexListComponent<I> {
    pub fn new(ordering: IndexListOrdering) -> Self {
        Self {
            index_list: vec![],
            selected_row: usize::MAX,
            list_ordering: ordering,
        }
    }

    pub fn update_index_list(&mut self, index_list: Vec<(String, I)>) {
        self.index_list = index_list;
        self.selected_row = usize::MAX;

        // Reorder list if needed
        if let IndexListOrdering::Alphabetical = self.list_ordering {
            self.index_list
                .sort_unstable_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        }
    }

    pub fn update<CB: FnMut(&str, I)>(&mut self, ui: &mut egui::Ui, on_element_selected: &mut CB) {
        let num_rows = self.index_list.len();
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
                            let (type_name, type_index) = &self.index_list[row_index];

                            if ui
                                .selectable_label(self.selected_row == row_index, type_name)
                                .clicked()
                            {
                                self.selected_row = row_index;
                                on_element_selected(type_name, *type_index);
                            }
                        }
                    });
            },
        );
    }
}

impl<I: Copy> Default for IndexListComponent<I> {
    fn default() -> Self {
        Self::new(IndexListOrdering::None)
    }
}
