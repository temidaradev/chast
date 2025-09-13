use std::error::Error;

slint::include_modules!();

pub struct Handler {}

impl Handler {
    pub fn handle_gui() -> Result<(), Box<dyn Error>> {
        let ui = AppWindow::new()?;

        ui.on_request_increase_value({
            let ui_handle = ui.as_weak();
            move || {
                let ui = ui_handle.unwrap();
                ui.set_counter(ui.get_counter() + 1);
            }
        });

        ui.run()?;

        Ok(())
    }
}
