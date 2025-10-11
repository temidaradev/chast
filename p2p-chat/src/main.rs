use crate::app::app::App;
use anyhow::Result;

pub mod app;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
fn main() -> Result<()> {
    match App::run() {
        Ok(()) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("GUI error: {}", e)),
    }
}
