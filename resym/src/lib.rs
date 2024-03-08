#[cfg(target_arch = "wasm32")]
mod frontend;
#[cfg(target_arch = "wasm32")]
mod mode;
#[cfg(target_arch = "wasm32")]
mod module_tree;
#[cfg(target_arch = "wasm32")]
mod module_tree_view;
#[cfg(target_arch = "wasm32")]
mod resym_app;
#[cfg(target_arch = "wasm32")]
mod settings;
#[cfg(target_arch = "wasm32")]
mod syntax_highlighting;
#[cfg(target_arch = "wasm32")]
mod ui_components;

#[cfg(target_arch = "wasm32")]
use memory_logger::blocking::MemoryLogger;

#[cfg(target_arch = "wasm32")]
use resym_app::ResymApp;

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::{self, prelude::*};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WebHandle {
    runner: eframe::WebRunner,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WebHandle {
    /// Installs a panic hook, then returns.
    #[allow(clippy::new_without_default)]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    /// Call this once from JavaScript to start your app.
    #[wasm_bindgen]
    pub async fn start(&self, canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
        let logger = MemoryLogger::setup(log::Level::Info).expect("application creation");

        self.runner
            .start(
                canvas_id,
                eframe::WebOptions::default(),
                Box::new(|cc| Box::new(ResymApp::new(cc, logger).expect("application creation"))),
            )
            .await
    }

    #[wasm_bindgen]
    pub fn destroy(&self) {
        self.runner.destroy();
    }

    /// The JavaScript can check whether or not your app has crashed:
    #[wasm_bindgen]
    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }

    #[wasm_bindgen]
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|s| s.message())
    }

    #[wasm_bindgen]
    pub fn panic_callstack(&self) -> Option<String> {
        self.runner.panic_summary().map(|s| s.callstack())
    }
}
