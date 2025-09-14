use std::error::Error;

slint::include_modules!();

pub struct App {}

impl App {
    pub fn run() -> Result<(), Box<dyn Error>> {
        let main = StartWindow::new()?;
        let main_handle = main.as_weak();

        let join = JoinWindow::new()?;
        let join_handle = join.as_weak();

        let create = CreateWindow::new()?;
        let create_handle = create.as_weak();

        let val = main_handle.clone();

        main.show();

        main.on_switch_to_join_window(move || {
            let main = val.unwrap();
            let join = join_handle.unwrap();
            join.show();
            main.hide();
        });

        main.on_switch_to_create_window(move || {
            let val = main_handle.clone();
            let main = val.unwrap();
            let create = create_handle.unwrap();
            create.show();
            main.hide();
        });

        main.run()?;

        Ok(())
    }
}
