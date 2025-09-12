use gtk4::{prelude::*, glib, ApplicationWindow, Application}

pub struct GUI{}

impl GUI {


fn handle_gui() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("P2P Chat")
        .build();

    window.present();
}

}
