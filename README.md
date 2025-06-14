https://github.com/user-attachments/assets/91c10772-1902-4b49-8e7d-8087634b22a4

# net

Rust library for communicating with peers using WebSockets and WebRTC.

Designed for games and realtime applications in the browser, which may not require every message to be ordered and reliable.

### Install 

Clone the repo, and then set as dependency in your `Cargo.toml`:

```bash
cargo add net --path net
```

### Usage

The API is extremely simple:
- ➡️ Send messages by calling .send_reliable() or .send_unreliable() on `Server`.
- 🔄 Monitor for messages by emptying an `EventQueue` using .pop_all() at whatever frequency your application requires.

Both objects are threadsafe and can be freely cloned.

```rust
use log::info;
use net::{Event, Server};

fn main() {
    // Server on port 3000 (tcp/udp)
    let (mut server, mut queue) = Server::new("127.0.0.1:3000");

    loop {
        // Pop events once every tick
        queue
            .pop_all()
            .into_iter()
            .for_each(handle_event);

        // Send message to peer with ID '0' reliably (using a web socket)
        server.send_reliable(0, "hello!".as_bytes().to_vec());
        
        // Send message to the same peer unreliably (using WebRTC data channel)
        server.send_unreliable(0, "hello again!".as_bytes().to_vec());

        // sleep(...)
    }
}

fn handle_event(event: Event) {
    match event {
        Event::Open(id) => info!("Connection opened for {}", id),
        Event::Closed(id) => info!("Connection closed for {}", id),
        Event::Received(id, message) => info!("Received {:?} from {}", message, id)
    }
}
```

### Motivation

With recent improvements in coding agents, there has been a surge in AI-generated web games. However, the multiplayer experience of these demonstrations still tends to be poor. 

This is usually, in part, down to the unnecessary overhead introduced by relying on websockets for communication. WebRTC provides a way to improve on this, supporting unordered and unreliable messaging, but is more complex to setup and usually beyond the abilities of current LLMs to get right. This library provides a simple way to do so, with sensible defaults.

### Limitations

- There is no SSL support, and consequently no `wss://` support, in this crate. It is recommended to use a reverse proxy, like nginx, to upgrade traffic.
- The usual maximum message size for both websockets and webrtc data channels apply, which may depend on the client implementation.

### Connecting as a client

This crate just operates on the server-side. For your frontend, you will need to setup connecting via websockets and accepting signalling messages that the server will automatically send. 

The [clients folder 📁](clients/) contains simple demonstrations for doing this.

<details>
    <summary>Sketch of the procedure for joining</summary>
    <ul>
        <li>Client opens websocket connection with the server. Server accepts.</li>
        <li>Server sends an SDP offer and ICE candidate(s) to Client in text-mode.</li>
        <li>Client sends an SDP answer and ICE candidate(s) to the Server in text-mode.</li>
        <li>If the above succeeds, an <code>Event::Open</code> is pushed to the event queue.</li>
        <li>Client/Server exchange messages over websockets in binary-mode, or using the webrtc datachannel.</li>
        <li>When either communication channel closes, an <code>Event::Closed</code> is pushed to the event queue.</li>
    </ul>
</details>

### License

MIT (except for parts which mention otherwise) 
