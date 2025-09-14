use std::error::Error;

slint::include_modules!();

pub struct Handler {}

impl Handler {
    pub fn handle_gui() -> Result<(), Box<dyn Error>> {
        let main = StartWindow::new()?;
        let main_handle = main.as_weak();
        let join = JoinWindow::new()?;
        let join_handle = join.as_weak();

        main.show();

        main.on_switch_to_join_window(move || {
            let main = main_handle.unwrap();
            let join = join_handle.unwrap();
            join.show();
            main.hide();
        });

        main.run()?;

        Ok(())
    }
}
