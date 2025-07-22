use iced::Center;
use iced::widget::{Column, button, column, text, text_input, scrollable};

#[derive(Default)]
pub struct GUI {
    pub input_value: String,
    pub display_message: String,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged(String),
    SendMessage,
    ClearMessage,
}

impl GUI {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::InputChanged(value) => {
                self.input_value = value;
            }
            Message::SendMessage => {
                if !self.input_value.trim().is_empty() {
                    self.messages.push(format!("You: {}", self.input_value));
                    self.display_message = format!("Sent: {}", self.input_value);
                    // Don't clear input_value here, let the main app handle it
                }
            }
            Message::ClearMessage => {
                self.display_message.clear();
                self.messages.clear();
                self.input_value.clear();
            }
        }
    }

    pub fn add_received_message(&mut self, message: String) {
        self.messages.push(message);
    }

    pub fn clear_input(&mut self) {
        self.input_value.clear();
    }

    pub fn view(&self) -> Column<'_, Message> {
        let mut message_list = column![];

        for msg in &self.messages {
            message_list = message_list.push(text(msg));
        }

        column![
            text("P2P Messaging").size(24),

            scrollable(message_list)
                .height(300),

            text_input("Type your message...", &self.input_value)
                .on_input(Message::InputChanged)
                .on_submit(Message::SendMessage)
                .padding(10),

            button("Send Message")
                .on_press(Message::SendMessage)
                .padding(10),

            if !self.display_message.is_empty() {
                column![
                    text(&self.display_message),
                    button("Clear Messages").on_press(Message::ClearMessage)
                ]
            } else {
                column![]
            }
        ]
            .padding(20)
            .spacing(10)
            .align_x(Center)
    }
}