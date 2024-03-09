use eframe::egui::{self, ScrollArea, TextStyle};
use resym_core::frontend::{TypeIndex, TypeList};

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
