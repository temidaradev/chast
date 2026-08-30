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

On Nix there's a flake, so you can grab everything with:

```bash
nix develop
cargo build --release
```

### 3. Run it

```bash
cargo run --bin p2p-chat
```

### Server Mode (Optional)

For a more traditional chat experience with message tracking and room management, you can run the server component. This provides additional features beyond pure peer-to-peer communication :3

It hosts as many rooms as you want and keeps everything in a SQLite file, so your rooms and everything said in them survive a reboot:

```bash
cargo run --bin server -- create general
cargo run --bin server -- list
cargo run --bin server -- serve
cargo run --bin server -- delete general
```

`serve` is what runs when you don't say otherwise. The database lives at `server.db` unless you point `P2P_CHAT_DB` somewhere else.

The server keeps its network identity in that same file, so its address and every room ticket stay exactly the same across restarts. A ticket you shared months ago still works after the machine reboots, and when people come back they only download the messages they actually missed.

Your client remembers things too, your identity key in `~/.config/p2p-chat/identity.key` and a copy of each room in `~/.config/p2p-chat/rooms/`. Open a chat and the history is already sitting there before the server even answers.

### Accounts

When the server starts it prints a **server address**, a long hex string. That's the one thing people need to sign up, and it's not the same thing as a room ticket. Think of it like this: the address gets you to the server, an account gets you a name, and an invite gets you into a room.

Accounts look like `@you:server`. Passwords are hashed with argon2 and every device gets its own token, so you can be signed in on your laptop and your phone at the same time without them fighting. Rooms have power levels like Matrix does (100 admin, 50 moderator, 0 member) and they're invite only unless you say otherwise.

Logins never touch the gossip network. They go over their own encrypted connection, so your password is never broadcast into a room :3 Every message is signed by the device that sent it, and only the server gets to decide which key belongs to which account, so nobody can post pretending to be you.

Running things from the server console:

```bash
cargo run --bin server -- register alice hunter2hunter2
cargo run --bin server -- create general alice
cargo run --bin server -- invite general bob
cargo run --bin server -- members general
cargo run --bin server -- grant general bob 50
```

There's also `chatctl`, a terminal client for when you don't feel like clicking:

```bash
cargo run --bin chatctl -- connect <server-address>
cargo run --bin chatctl -- register alice hunter2hunter2
cargo run --bin chatctl -- rooms
cargo run --bin chatctl -- send general "hello"
cargo run --bin chatctl -- read general
```

### No account needed

Accounts are completely optional! The sign in window has a second half for plain old peer to peer: type a name, hit **Create direct room**, and share the ticket. Or paste a friend's ticket and hit **Join direct room** instead.

No server, no signup, exactly how the app has always worked. Nothing gets saved for you, names are whatever people say they are, and the room is gone once everyone closes it.

You can also install pre-built executables for both the chat GUI and server from the [releases page](https://github.com/temidaradev/p2p-vpn-rust/releases).

That's it!

## How to use

### Starting a chat room

1. Run the application
2. In the lower half of the window, type your name
3. Leave the ticket box empty and click "**Create direct room**"
4. **Copy the ticket** with the button in the top bar
5. Share this ticket with friends you want to chat with!

### Joining someone's room

1. Get a ticket from a friend
2. Run the application
3. Type your name and paste the ticket in the lower half
4. Click "**Join direct room**"
5. Start chatting!

### Using an account instead

1. Ask whoever runs the server for its **server address**
2. Paste it in the top box and pick a username and password
3. Hit "**Create account**" the first time, "**Sign in**" after that
4. You land on your room list, where you can open a room or make one of your own
5. Invite only rooms need someone to invite you first

You only have to paste that address once. Next time you open the app it fills itself in and signs you back in on its own 🎉

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
- **`server/`** - Optional server to break the rules of p2p :3, holds the accounts and the rooms that stick around
- **`client/`** - Shared client bits, your identity, your saved session, and talking to a server
- **`chatctl/`** - Terminal client for people who like terminals
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
