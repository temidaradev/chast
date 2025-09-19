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

        let chat = ChatWindow::new()?;
        let chat_handle = chat.as_weak();

        let val = main_handle.clone();
        let val_join = join_handle.clone();
        let val_create = create_handle.clone();
        let val_chat = chat_handle.clone();

        main.show();

        main.on_switch_to_join_window(move || {
            let main = val.unwrap();
            let join = val_join.unwrap();

            join.show();
            main.hide();
        });

        main.on_switch_to_create_window(move || {
            let val = main_handle.clone();
            let main = val.unwrap();
            let create = val_create.unwrap();

            create.show();
            main.hide();
        });

        join.on_switch_to_chat_window(move |_, _| {
            let val_join = join_handle.clone();
            let join = val_join.unwrap();
            let chat = chat_handle.unwrap();

            chat.show();
            join.hide();
        });

        create.on_switch_to_chat_window(move |_| {
            let val_create = create_handle.clone();
            let create = val_create.unwrap();
            let chat = val_chat.unwrap();

            chat.show();
            create.hide();
        });

        main.run()?;

        Ok(())
    }
}
