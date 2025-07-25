fn main() {
    handle_gui().unwrap();
}

fn handle_gui() -> iced::Result {
    iced::run("P2P VPN", gui::App::update, gui::App::view)
}
