use iced::Center;
use iced::widget::{Column, button, column, container, text, text_input};
// use iroh::{Endpoint, NodeId};
// use iroh_gossip::api::GossipReceiver;
// use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
// use std::collections::HashMap;

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
    ChatInputChanged(String),
    SendMessage,
}

pub struct App {
    page: Page,
    join: Join,
    pub topic: Option<TopicId>,
    pub chat_input: String,
    pub messages: Vec<String>,
    // endpoint: Option<Endpoint>,
    // gossip: Option<Gossip>,
    // receiver: Option<GossipReceiver>,
    // names: HashMap<NodeId, String>,
    // messages: Vec<(String, String)>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::Menu,
            join: Join::default(),
            topic: None,
            chat_input: String::new(),
            messages: Vec::new(),
        }
    }
}

impl App {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::GoToJoinRoom => self.page = Page::JoinRoom,
            Message::GoToMenu => {
                self.page = Page::Menu;
                self.topic = None;
            }
            Message::TokenChanged(token) => self.join.token = token,
            Message::CreateRoom => {
                let topic = TopicId::from_bytes(rand::random());
                self.topic = Some(topic.clone());
                self.messages.clear();
                self.page = Page::ChatRoom;
                println!("Created room: {:?}", topic);
            }
            Message::JoinRoom => {
                println!("Attempting to join room with token: {}", self.join.token);
                if let Some(topic) = hex::decode(&self.join.token)
                    .ok()
                    .and_then(|b| if b.len() == 32 { Some(b) } else { None })
                    .map(|b| TopicId::from_bytes(b.try_into().unwrap()))
                {
                    println!("Successfully joined room: {:?}", topic);
                    self.topic = Some(topic);
                    self.page = Page::ChatRoom;
                }
            }
            Message::GoToChatRoom => self.page = Page::ChatRoom,
            Message::ChatInputChanged(input) => self.chat_input = input,
            Message::SendMessage => {
                let msg = self.chat_input.trim();
                if !msg.is_empty() {
                    self.messages.push(msg.to_string());
                    self.chat_input.clear();
                }
            }
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
                use iced::widget::{Space, container, row, scrollable};
                let messages_view = if self.messages.is_empty() {
                    column![text("No messages yet.")].spacing(10).padding(10)
                } else {
                    self.messages
                        .iter()
                        .enumerate()
                        .fold(column![], |col, (i, msg)| {
                            let bubble = container(text(msg).size(18)).padding(12);
                            col.push(bubble)
                        })
                        .spacing(8)
                        .padding(8)
                };

                let scroll = scrollable(messages_view)
                    .height(iced::Length::FillPortion(3))
                    .width(iced::Length::Fill);

                let input_row = row![
                    text_input("Type a message...", &self.chat_input)
                        .on_input(Message::ChatInputChanged)
                        .on_submit(Message::SendMessage)
                        .padding(10)
                        .size(18)
                        .width(iced::Length::FillPortion(4)),
                    button("Send")
                        .on_press(Message::SendMessage)
                        .padding([10, 18]),
                ]
                .spacing(10)
                .width(iced::Length::Fill);

                column![
                    text("Chat Room").size(36),
                    text(format!("Room Token: {}", token_str)).size(16),
                    Space::with_height(10),
                    container(scroll)
                        .height(iced::Length::FillPortion(3))
                        .width(iced::Length::Fill)
                        .padding(8),
                    Space::with_height(10),
                    input_row,
                    Space::with_height(10),
                    button("Back to Menu")
                        .on_press(Message::GoToMenu)
                        .padding([8, 16]),
                ]
                .spacing(8)
                .padding(24)
                .align_x(Center)
            }
        }
    }
}
