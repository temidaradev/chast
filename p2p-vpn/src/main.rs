fn main() {
    handle_gui().unwrap();
}

fn handle_gui() -> iced::Result {
    iced::run("P2P Chat Rust", gui::App::update, gui::App::view)
}
