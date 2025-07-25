use iced::Center;
use iced::widget::{Column, button, column, text};

#[derive(Default)]
pub struct Messaging {
    income: String,
    message: String,
}

pub struct Register {
    selection: i32,
    username: String,
}

pub struct Join {
    token: String,
    selection: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Increment,
    Decrement,
}

impl Messaging {
    pub fn update(&mut self, message: Message) {
        todo!()
    }

    pub fn view(&self) -> Column<'_, Message> {
        column![
            button("Increment").on_press(Message::Increment),
            // text(self.value).size(50),
            button("Decrement").on_press(Message::Decrement)
        ]
            .padding(20)
            .align_x(Center)
    }
}