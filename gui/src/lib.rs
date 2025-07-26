use iced::Center;
use iced::widget::{Column, button, column, text, text_input};
use iroh_gossip::proto::{TopicId, topic};

#[derive(Default)]
pub struct Join {
    pub token: String,
}

#[derive(Debug, Clone)]
pub enum Page {
    Menu,
    JoinRoom,
    ChatRoom,
}

#[derive(Debug, Clone)]
pub enum Message {
    GoToJoinRoom,
    GoToMenu,
    TokenChanged(String),
    CreateRoom,
    JoinRoom,
    GoToChatRoom,
}

pub struct App {
    page: Page,
    join: Join,
    pub topic: Option<TopicId>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::Menu,
            join: Join::default(),
            topic: None,
        }
    }
}

impl App {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::GoToJoinRoom => self.page = Page::JoinRoom,
            Message::GoToMenu => self.page = Page::Menu,
            Message::TokenChanged(token) => self.join.token = token,
            Message::CreateRoom => {
                let topic = TopicId::from_bytes(rand::random());
                self.topic = Some(topic.clone());
                self.page = Page::ChatRoom;
                println!("Created room: {:?}", topic);
            }
            Message::JoinRoom => {
                if let Some(topic) = hex::decode(&self.join.token)
                    .ok()
                    .and_then(|b| if b.len() == 32 { Some(b) } else { None })
                    .map(|b| TopicId::from_bytes(b.try_into().unwrap()))
                {
                    self.topic = Some(topic);
                    self.page = Page::ChatRoom;
                }
            }
            Message::GoToChatRoom => self.page = Page::ChatRoom,
        }
    }

    pub fn view(&self) -> Column<'_, Message> {
        match self.page {
            Page::Menu => column![
                button("Create Room").on_press(Message::CreateRoom),
                button("Join Room").on_press(Message::GoToJoinRoom),
            ]
            .padding(20)
            .align_x(Center),

            Page::JoinRoom => column![
                text("Enter Room Token:"),
                text_input("Token...", &self.join.token).on_input(Message::TokenChanged),
                button("Join").on_press(Message::JoinRoom),
                button("Back").on_press(Message::GoToMenu),
            ]
            .padding(20)
            .align_x(Center),

            Page::ChatRoom => {
                let token_str = self
                    .topic
                    .as_ref()
                    .map(|t| hex::encode(t.as_bytes()))
                    .unwrap_or_else(|| "No token".to_string());
                column![
                    text("Chat Room").size(32),
                    text(format!("Room Token: {}", token_str)).size(16),
                    text("Will add messaging to gui"),
                    button("Back to Menu").on_press(Message::GoToMenu),
                ]
                .padding(20)
                .align_x(Center)
            }
        }
    }
}
