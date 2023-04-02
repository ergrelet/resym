use eframe::egui;
use resym_core::pdb_types::PrimitiveReconstructionFlavor;

use crate::settings::ResymAppSettings;

pub struct SettingsComponent {
    window_open: bool,
    pub app_settings: ResymAppSettings,
}

impl SettingsComponent {
    pub fn new(app_settings: ResymAppSettings) -> Self {
        Self {
            window_open: false,
            app_settings,
        }
    }

    pub fn open(&mut self) {
        self.window_open = true;
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        egui::Window::new("Settings")
            .anchor(egui::Align2::CENTER_CENTER, [0.0; 2])
            .open(&mut self.window_open)
            .auto_sized()
            .collapsible(false)
            .show(ctx, |ui| {
                const INTER_SECTION_SPACING: f32 = 10.0;
                ui.label("Theme");
                // Show radio-buttons to switch between light and dark mode.
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.app_settings.use_light_theme, true, "☀ Light");
                    ui.selectable_value(&mut self.app_settings.use_light_theme, false, "🌙 Dark");
                });
                ui.label(
                    egui::RichText::new("Font size")
                        .color(ui.style().visuals.widgets.inactive.text_color()),
                );
                egui::ComboBox::from_id_source("font_size")
                    .selected_text(format!("{}", self.app_settings.font_size))
                    .show_ui(ui, |ui| {
                        for font_size in 8..=20 {
                            ui.selectable_value(
                                &mut self.app_settings.font_size,
                                font_size,
                                font_size.to_string(),
                            );
                        }
                    });
                ui.add_space(INTER_SECTION_SPACING);

                ui.label("Search");
                ui.checkbox(
                    &mut self.app_settings.search_case_insensitive,
                    "Case insensitive",
                );
                ui.checkbox(
                    &mut self.app_settings.search_use_regex,
                    "Enable regular expressions",
                );
                ui.add_space(INTER_SECTION_SPACING);

                ui.label("Type reconstruction");
                ui.checkbox(
                    &mut self.app_settings.enable_syntax_hightlighting,
                    "Enable C++ syntax highlighting",
                );

                ui.label(
                    egui::RichText::new("Primitive types style")
                        .color(ui.style().visuals.widgets.inactive.text_color()),
                );
                egui::ComboBox::from_id_source("primitive_types_flavor")
                    .selected_text(format!("{:?}", self.app_settings.primitive_types_flavor))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.app_settings.primitive_types_flavor,
                            PrimitiveReconstructionFlavor::Portable,
                            "Portable",
                        );
                        ui.selectable_value(
                            &mut self.app_settings.primitive_types_flavor,
                            PrimitiveReconstructionFlavor::Microsoft,
                            "Microsoft",
                        );
                        ui.selectable_value(
                            &mut self.app_settings.primitive_types_flavor,
                            PrimitiveReconstructionFlavor::Raw,
                            "Raw",
                        );
                    });

                ui.checkbox(&mut self.app_settings.print_header, "Print header");
                ui.checkbox(
                    &mut self.app_settings.reconstruct_dependencies,
                    "Print definitions of referenced types",
                );
                ui.checkbox(
                    &mut self.app_settings.print_access_specifiers,
                    "Print access specifiers",
                );
                ui.checkbox(
                    &mut self.app_settings.print_line_numbers,
                    "Print line numbers",
                );
            });
    }
}
