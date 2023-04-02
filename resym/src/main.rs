#![windows_subsystem = "windows"]

mod frontend;
mod mode;
mod resym_app;
mod settings;
mod syntax_highlighting;
mod ui_components;

use anyhow::{anyhow, Result};
use memory_logger::blocking::MemoryLogger;

use resym_app::ResymApp;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> Result<()> {
    let logger = MemoryLogger::setup(log::Level::Info)?;

    let native_options = eframe::NativeOptions {
        icon_data: load_icon(),
        ..Default::default()
    };

    eframe::run_native(
        PKG_NAME,
        native_options,
        Box::new(|cc| Box::new(ResymApp::new(cc, logger).expect("application creation"))),
    )
    .map_err(|err| anyhow!("eframe::run_native failed: {err}"))
}

/// Load an icon to display on the application's windows.
/// Note: only available on Windows
#[cfg(windows)]
fn load_icon() -> Option<eframe::IconData> {
    const ICON_WIDTH: u32 = 96;
    const ICON_HEIGHT: u32 = 96;
    const ICON_BYTES_PER_PIXEL: usize = 4;
    const ICON_BYTE_SIZE: usize = ICON_WIDTH as usize * ICON_HEIGHT as usize * ICON_BYTES_PER_PIXEL;
    const ICON_BYTES: &[u8; ICON_BYTE_SIZE] = include_bytes!("../resources/resym_96.bin");

    Some(eframe::IconData {
        rgba: ICON_BYTES.to_vec(),
        width: ICON_WIDTH,
        height: ICON_HEIGHT,
    })
}

/// Load an icon to display on the application's windows.
/// Note: only available on Windows
#[cfg(not(windows))]
fn load_icon() -> Option<eframe::IconData> {
    None
}
