# Chast - P2P Chat Application

**Welcome to Chast!** A modern, peer-to-peer chat application built with Rust that lets you create secure, direct connections with friends without needing any central servers.

## What you'll need

Make sure you have:

- **Rust** - The latest stable version ([get it here](https://www.rust-lang.org/))
- **An internet connection** - For initial peer discovery (then it's all direct!)
- **A friend to chat with** - Because chatting alone isn't as fun 😄

## Getting started

### 1. Clone the repo

```bash
git clone https://github.com/temidaradev/p2p-vpn-rust.git
cd p2p-vpn-rust
```

### 2. Build it

```bash
cargo build --release
```

_Grab a coffee - this might take a few minutes the first time as Rust downloads and compiles all dependencies..._

### 3. Run it

```bash
cargo run --bin p2p-chat
```

### Server Mode (Optional)

For a more traditional chat experience with message tracking and room management, you can run the server component. This provides additional features beyond pure peer-to-peer communication :3

```bash
cargo run --bin server
```

You can also install pre-built executables for both the chat GUI and server from the [releases page](https://github.com/temidaradev/p2p-vpn-rust/releases).

That's it!

## How to use

### Starting a chat room

1. Run the application
2. Click "**Create Room**"
3. Enter your username
4. Click "**Create**"
5. **Copy the ticket** this is your room's "ticket"
6. Share this ticket with friends you want to chat with!

### Joining someone's room

1. Get a ticket from a friend
2. Run the application
3. Click "**Join Room**"
4. Enter your username
5. Paste the ticket
6. Click "**Join**"
7. Start chatting!

If you want to enter the server im hosting on my server, you can join by pasting this room id into related section:

```
pmrhi33qnfrseos3ge4dmlbrga4symrrgawdcnrvfqzdeobmgm4synjtfqzdalbrg42cymjtguwdmnzmgiydelbrgazcymjrhewdcmrrfq2dklbsgaycynbrfqzdamzmgi2tglbrg4wdcmjyfqzdknjmgu4cymjvhawdonjmgizdelbxgawdmojmgiydglbrhazcymzwluwce3tpmrsxgir2ln5se3tpmrsv62leei5centgmzsdimbtgu3tqztfg5tdqzjumnrtqobzgm4tkmzxgfqtgmdcgm4wemdbge4tsyrshbsgmntbmfswim3bmzsdgnlemuzdanrqgftcelbcojswyylzl52xe3bchirgq5duobztulzpojswyylzfzuxe33ifzwgs3tlfyxselbcmruxezldorpwczdeojsxg43fomrduw25pvox2
```

## How it works under the hood

Curious about the magic? Here's the simplified version:

1. **Room Creation**: When you create a room, your computer becomes a "host" with a unique network address
2. **Ticket Generation**: The app creates a special "ticket" containing your network address and a secret room ID
3. **Direct Connection**: When someone uses your ticket, their app connects directly to yours
4. **Peer-to-Peer**: From then on, messages flow directly between devices using [Iroh](https://iroh.computer/)

No servers, no data collection, no corporate oversight

But using the server option breaks this...

## Project structure

This is a Rust workspace with several components:

- **`p2p-chat/`** - The main GUI application (what you actually run)
- **`server/`** - Optional server to break the rules of p2p :3
- **`messaging/`** - Message format and serialization logic
- **`ticket/`** - Room "ticket" encoding/decoding
- **`target/`** - Compiled binaries (created when you build)

## Contributing

Found a bug? Have an idea? Contributions are welcome! Feel free to:

- Open an issue for bugs or feature requests
- Submit a pull request with improvements
- Share feedback on the user experience

## Why "Chast"?

**Chat** + **Rust** = **Chast**

---

_Built with ❤️ in Rust. by temidaradev!_ 🦀
