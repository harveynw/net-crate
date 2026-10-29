
https://github.com/user-attachments/assets/91c10772-1902-4b49-8e7d-8087634b22a4

<small><i>Multiplayer demo adapted from [this](https://threejs.org/examples/#webgl_animation_skinning_blending) THREE.js example.</i></small>

# net crate

Rust server that handles network communication with clients running in the browser. It supports two modes:
- Standard <b>_reliable_, _ordered_</b> messaging using WebSockets.
- An alternative <b>_unreliable_, _unordered_</b> messaging behaviour using WebRTC data channels. 

The latter is helpful for avoiding latency spikes in applications where ordering and delivery guarantees are less important, such as character movement in multiplayer games. 

This is achieved using WebRTC, which is now well supported across browsers. It has a relatively large and complex API compared to WebSockets and supports numerous use cases. Thus this crate serves as a convenient implementation of the specific case where you wish to send small buffers of arbitrary bytes at high frequency.

This is just the server component in a wider game system, you will need to arrange the necessary signalling logic on the client side. You can either view some examples connectors under `examples_clients` or ask a coding agent to write one.

### Install 

Simplest strategy is to clone the repo, and then set as dependency in your `Cargo.toml`:

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
    // WebSocket server on TCP port 3000; WebRTC uses ephemeral UDP ports
    let (mut server, mut queue) = Server::new("127.0.0.1:3000");

    loop {
        // Pop events once every tick
        queue
            .pop_all()
            .into_iter()
            .for_each(handle_event);

        // Send message to peer with ID '0' reliably (using a web socket)
        server.send_reliable(0, "hello!".as_bytes().into());
        
        // Send message to the same peer unreliably (using WebRTC data channel)
        server.send_unreliable(0, "hello again!".as_bytes().into());

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

### Architecture

```
 Browser                                         Server (This Crate)
                                                                    
                                                                    
 WebSocket Client                                WebSocket Server   
┌──────────────────┐                            ┌──────────────────┐
│ Binary           ◄───Game Data (Reliable)─────► Binary           │
└──────────────────┘                            └──────────────────┘
┌──────────────────┐                            ┌──────────────────┐
│ Text             ◄───WebRTC Signalling────────► Text             │
└──────────────────┘                            └──────────────────┘
                                                                    
                                                                    
 Web RTC DataChannel                             Web RTC DataChannel
┌──────────────────┐                            ┌──────────────────┐
│ Binary           ◄───Game Data (Unreliable)───► Binary           │
└──────────────────┘                            └──────────────────┘
```

The WebSocket listener uses the configured TCP port. Each WebRTC peer binds its own ephemeral UDP ports. 

### Limitations

- There is no SSL support, and consequently no `wss://` support, in this crate. It is recommended to use a reverse proxy, like nginx, to upgrade traffic.

### License

MIT (except for parts which mention otherwise) 
