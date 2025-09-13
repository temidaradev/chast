use std::error::Error;

slint::include_modules!();

pub struct Handler {}

impl Handler {
    pub fn handle_gui() -> Result<(), Box<dyn Error>> {
        let ui = AppWindow::new()?;

        ui.run()?;

        Ok(())
    }
}
