# P2P Chat in Rust

A lightweight peer-to-peer CLI chat application written in Rust. It allows users to create and join chat rooms without relying on a central server.

## Requirements

- [Rust](https://www.rust-lang.org/) (latest stable version recommended)
- Cargo (comes bundled with Rust)
- A terminal or shell to run the commands
- Internet connection for peers to connect with each other

## Installation

Clone the repository:

```sh
git clone https://github.com/temidaradev/rust-p2p-chat.git
cd rust-p2p-chat
```

Or install from release:

[v0.1](https://github.com/temidaradev/rust-p2p-chat/releases/tag/v0.1)

Build the project:

```sh
cargo build --release
```

The compiled binary will be available at: `target/release/p2p-chat`

You can run it directly: `./target/release/p2p-chat --help`

Or continue from [Usage](https://github.com/temidaradev/rust-p2p-chat/blob/main/README.md#usage)

## How it works

- One user opens a chat room and receives a unique ticket.
- Other users use this ticket to connect directly to the host.
- Messages are exchanged peer-to-peer in real time.

## Usage

Start a new chat room:

```sh
cargo run -- --name user1 open
```

This will generate a ticket that you can share with others.

Join an existing chat room (replace <ticket> with the one provided):

```sh
cargo run -- --name user2 join <ticket>
```

Now both users are connected directly and can chat in real time.

Test:

[Screencast_20250822_225853.webm](https://github.com/user-attachments/assets/1325c830-45b2-4e6a-bf31-a450a923bb86)
