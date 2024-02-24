use eframe::egui;
use resym_core::backend::{Backend, BackendCommand};

use crate::resym_app::ResymPDBSlots;

pub struct OpenURLComponent {
    url_text: String,
    pdb_slot: Option<ResymPDBSlots>,
}

impl OpenURLComponent {
    pub fn new() -> Self {
        Self {
            url_text: String::default(),
            pdb_slot: None,
        }
    }

    pub fn open(&mut self, pdb_slot: ResymPDBSlots) {
        self.pdb_slot = Some(pdb_slot);
    }

    fn close(&mut self) {
        self.pdb_slot = None;
        self.url_text.clear();
    }

    pub fn update(&mut self, ctx: &egui::Context, backend: &Backend) {
        if let Some(pdb_slot) = self.pdb_slot {
            egui::Window::new("Open URL")
                .anchor(egui::Align2::CENTER_CENTER, [0.0; 2])
                .auto_sized()
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("URL to open:");
                    ui.text_edit_singleline(&mut self.url_text);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.close();
                        } else if ui.button("Open").clicked() {
                            self.start_open_pdb_from_url(backend, pdb_slot, self.url_text.clone());
                            self.close();
                        }
                    })
                });
        }
    }

    fn start_open_pdb_from_url(&self, backend: &Backend, pdb_slot: ResymPDBSlots, url: String) {
        if let Err(err) = backend.send_command(BackendCommand::LoadPDBFromURL(pdb_slot.into(), url))
        {
            log::error!("Failed to load URL: {err}");
        }
    }
}
