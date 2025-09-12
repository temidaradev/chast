use gtk4::{glib, prelude::*, Application, ApplicationWindow};

pub struct gui {}

pub const APP_ID: &str = "com.temidaradev.p2p_chat";

impl gui {
    pub fn handle_gui() -> glib::ExitCode {
        let app = Application::builder().application_id(APP_ID).build();
        app.connect_activate(build_ui);
        app.run()
    }
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("P2P Chat")
        .build();

    window.present();
}
