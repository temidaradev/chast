# P2P Chat in Rust

A lightweight peer-to-peer CLI chat application written in Rust. It allows users to create and join chat rooms without relying on a central server.

## Requirements
- [Rust](https://www.rust-lang.org/) (latest stable version recommended)  
- Cargo (comes bundled with Rust)  
- A terminal or shell to run the commands  
- Internet connection for peers to connect with each other


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
