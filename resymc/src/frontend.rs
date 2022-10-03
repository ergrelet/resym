use crossbeam_channel::{Receiver, Sender};
use resym_core::{
    frontend::{FrontendCommand, FrontendController},
    Result, ResymCoreError,
};

/// Frontend implementation for the CLI application
/// This struct enables the backend to communicate with us (the frontend)
pub struct CLIFrontendController {
    pub rx_ui: Receiver<FrontendCommand>,
    tx_ui: Sender<FrontendCommand>,
}

impl FrontendController for CLIFrontendController {
    /// Used by the backend to send us commands and trigger a UI update
    fn send_command(&self, command: FrontendCommand) -> Result<()> {
        self.tx_ui
            .send(command)
            .map_err(|err| ResymCoreError::CrossbeamError(err.to_string()))
    }
}

impl CLIFrontendController {
    pub fn new(tx_ui: Sender<FrontendCommand>, rx_ui: Receiver<FrontendCommand>) -> Self {
        Self { rx_ui, tx_ui }
    }
}
