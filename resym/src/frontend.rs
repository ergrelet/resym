use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Sender};
use eframe::epi;
use resym_core::frontend::{FrontendCommand, FrontendController};

use std::sync::RwLock;

/// This struct enables the backend to communicate with us (the frontend)
pub struct EguiFrontendController {
    pub rx_ui: Receiver<FrontendCommand>,
    tx_ui: Sender<FrontendCommand>,
    ui_frame: RwLock<Option<epi::Frame>>,
}

impl FrontendController for EguiFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        self.tx_ui.send(command)?;
        // Force the UI backend to call our app's update function on the other end
        if let Ok(ui_frame_opt) = self.ui_frame.try_read() {
            if let Some(ui_frame) = ui_frame_opt.as_ref() {
                ui_frame.request_repaint();
            }
        }
        Ok(())
    }
}

impl EguiFrontendController {
    pub fn new(tx_ui: Sender<FrontendCommand>, rx_ui: Receiver<FrontendCommand>) -> Self {
        Self {
            rx_ui,
            tx_ui,
            ui_frame: RwLock::new(None),
        }
    }

    pub fn set_ui_frame(&self, ui_frame: epi::Frame) -> Result<()> {
        match self.ui_frame.write() {
            Err(_) => Err(anyhow!("Failed to update `ui_frame`")),
            Ok(mut ui_frame_opt) => {
                *ui_frame_opt = Some(ui_frame);
                Ok(())
            }
        }
    }
}
