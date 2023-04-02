#[cfg(target_arch = "wasm32")]
mod frontend;
#[cfg(target_arch = "wasm32")]
mod mode;
#[cfg(target_arch = "wasm32")]
mod resym_app;
#[cfg(target_arch = "wasm32")]
mod settings;
#[cfg(target_arch = "wasm32")]
mod syntax_highlighting;
#[cfg(target_arch = "wasm32")]
mod ui_components;

#[cfg(target_arch = "wasm32")]
use eframe::web::AppRunnerRef;
#[cfg(target_arch = "wasm32")]
use memory_logger::blocking::MemoryLogger;

#[cfg(target_arch = "wasm32")]
use resym_app::ResymApp;

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::{self, prelude::*};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WebHandle {
    handle: AppRunnerRef,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WebHandle {
    #[wasm_bindgen]
    pub fn stop_web(&self) -> Result<(), wasm_bindgen::JsValue> {
        let mut app = self.handle.lock();
        app.destroy()
    }

    #[wasm_bindgen]
    pub fn set_some_content_from_javasript(&mut self, _some_data: &str) {
        let _app = self.handle.lock().app_mut::<ResymApp>();
        // _app.data = some_data;
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_wasm_hooks() {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn start_separate(canvas_id: &str) -> Result<WebHandle, wasm_bindgen::JsValue> {
    let logger = MemoryLogger::setup(log::Level::Info).expect("application creation");
    let web_options = eframe::WebOptions::default();

    eframe::start_web(
        canvas_id,
        web_options,
        Box::new(|cc| Box::new(ResymApp::new(cc, logger).expect("application creation"))),
    )
    .await
    .map(|handle| WebHandle { handle })
}

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn start(canvas_id: &str) -> Result<WebHandle, wasm_bindgen::JsValue> {
    init_wasm_hooks();

    start_separate(canvas_id).await
}
