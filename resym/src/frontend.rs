use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use resym_core::{
    frontend::{FrontendCommand, FrontendController},
    Result, ResymCoreError,
};

/// This struct enables the backend to communicate with us (the frontend)
pub struct EguiFrontendController {
    pub rx_ui: Receiver<FrontendCommand>,
    tx_ui: Sender<FrontendCommand>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    egui_ctx: egui::Context,
}

impl FrontendController for EguiFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        self.tx_ui
            .send(command)
            .map_err(|err| ResymCoreError::CrossbeamError(err.to_string()))?;

        // Force the UI backend to call our app's update function on the other end.
        // Note(ergrelet): not available for wasm32 targets (multi-threading is limited).
        #[cfg(not(target_arch = "wasm32"))]
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
