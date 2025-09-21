use crate::app::app::App;
use anyhow::Result;

pub mod app;

fn main() -> Result<()> {
    match App::run() {
        Ok(()) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("GUI error: {}", e)),
    }
}
