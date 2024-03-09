use eframe::egui::{self, ScrollArea, TextStyle};
use memory_logger::blocking::MemoryLogger;

pub struct ConsoleComponent {
    logger: &'static MemoryLogger,
    content: Vec<String>,
}

impl ConsoleComponent {
    pub fn new(logger: &'static MemoryLogger) -> Self {
        Self {
            logger,
            content: vec![],
        }
    }

    pub fn update(&mut self, ui: &mut egui::Ui) {
        // Update console content
        self.content
            .extend(self.logger.read().lines().map(|s| s.to_string()));
        self.logger.clear();

        const TEXT_STYLE: TextStyle = TextStyle::Monospace;
        let row_height = ui.text_style_height(&TEXT_STYLE);
        let num_rows = self.content.len();
        ScrollArea::both().stick_to_bottom(true).show_rows(
            ui,
            row_height,
            num_rows,
            |ui, row_range| {
                for row_index in row_range {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.content[row_index].as_str())
                            .font(TEXT_STYLE)
                            .clip_text(false),
                    );
                }
            },
        );
    }
}
