use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use resym_core::frontend::{FrontendCommand, FrontendController};

/// This struct enables the backend to communicate with us (the frontend)
pub struct EguiFrontendController {
    pub rx_ui: Receiver<FrontendCommand>,
    tx_ui: Sender<FrontendCommand>,
    egui_ctx: egui::Context,
}

impl FrontendController for EguiFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        self.tx_ui.send(command)?;
        // Force the UI backend to call our app's update function on the other end
        self.egui_ctx.request_repaint();
        Ok(())
    }
}

impl EguiFrontendController {
    pub fn new(
        tx_ui: Sender<FrontendCommand>,
        rx_ui: Receiver<FrontendCommand>,
        egui_ctx: egui::Context,
    ) -> Self {
        Self {
            rx_ui,
            tx_ui,
            egui_ctx,
        }
    }
}
