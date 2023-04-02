use eframe::egui;
use resym_core::syntax_highlighting::CodeTheme;

use crate::{mode::ResymAppMode, settings::ResymAppSettings, syntax_highlighting::highlight_code};

pub struct CodeViewComponent {}

impl CodeViewComponent {
    pub fn new() -> Self {
        Self {}
    }

    pub fn update(
        &mut self,
        app_settings: &ResymAppSettings,
        current_mode: &ResymAppMode,
        ui: &mut egui::Ui,
    ) {
        const LANGUAGE_SYNTAX: &str = "cpp";
        let theme = if app_settings.use_light_theme {
            CodeTheme::light(app_settings.font_size, LANGUAGE_SYNTAX.to_string())
        } else {
            CodeTheme::dark(app_settings.font_size, LANGUAGE_SYNTAX.to_string())
        };

        let line_desc = if let ResymAppMode::Comparing(_, _, _, line_changes, _) = current_mode {
            Some(line_changes)
        } else {
            None
        };

        // Layouter that'll disable wrapping and apply syntax highlighting if needed
        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
            let layout_job = highlight_code(
                ui.ctx(),
                &theme,
                string,
                app_settings.enable_syntax_hightlighting,
                line_desc,
            );
            ui.fonts(|fonts| fonts.layout_job(layout_job))
        };

        // Type dump area
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // TODO(ergrelet): see if there's a better way to compute this width.
                let line_number_digit_width = 2 + app_settings.font_size as u32;
                let (num_colums, min_column_width) = if app_settings.print_line_numbers {
                    match current_mode {
                        ResymAppMode::Comparing(_, _, last_line_number, ..) => {
                            // Compute the columns' sizes from the number of digits
                            let char_count = last_line_number.checked_ilog10().unwrap_or(1) + 1;
                            let line_number_width = (char_count * line_number_digit_width) as f32;

                            // Old index + new index + code editor
                            (3, line_number_width)
                        }
                        ResymAppMode::Browsing(_, last_line_number, _) => {
                            // Compute the columns' sizes from the number of digits
                            let char_count = last_line_number.checked_ilog10().unwrap_or(1) + 1;
                            let line_number_width = (char_count * line_number_digit_width) as f32;

                            // Line numbers + code editor
                            (2, line_number_width)
                        }
                        _ => {
                            // Code editor only
                            (1, 0.0)
                        }
                    }
                } else {
                    // Code editor only
                    (1, 0.0)
                };

                egui::Grid::new("code_editor_grid")
                    .num_columns(num_colums)
                    .min_col_width(min_column_width)
                    .show(ui, |ui| {
                        match current_mode {
                            ResymAppMode::Comparing(
                                line_numbers_old,
                                line_numbers_new,
                                _,
                                _,
                                reconstructed_type_diff,
                            ) => {
                                // Line numbers
                                if app_settings.print_line_numbers {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers_old.as_str())
                                            .font(egui::FontId::monospace(
                                                app_settings.font_size as f32,
                                            ))
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers_new.as_str())
                                            .font(egui::FontId::monospace(
                                                app_settings.font_size as f32,
                                            ))
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                }
                                // Text content
                                ui.add(
                                    egui::TextEdit::multiline(
                                        &mut reconstructed_type_diff.as_str(),
                                    )
                                    .code_editor()
                                    .layouter(&mut layouter),
                                );
                            }
                            ResymAppMode::Browsing(line_numbers, _, reconstructed_type_content) => {
                                // Line numbers
                                if app_settings.print_line_numbers {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut line_numbers.as_str())
                                            .font(egui::FontId::monospace(
                                                app_settings.font_size as f32,
                                            ))
                                            .interactive(false)
                                            .desired_width(min_column_width),
                                    );
                                }
                                // Text content
                                ui.add(
                                    egui::TextEdit::multiline(
                                        &mut reconstructed_type_content.as_str(),
                                    )
                                    .code_editor()
                                    .layouter(&mut layouter),
                                );
                            }
                            ResymAppMode::Idle => {}
                        }
                    });
            });
    }
}
