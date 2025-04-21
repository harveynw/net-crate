# net

```rust
use net::{EventQueue, Server};

fn main() {
    let (mut server: Server, mut queue: EventQueue) = Server::new("127.0.0.1:3000");

    // Enter event loop ...

    // Pop messages received
    let events: Vec<_> = queue.pop_all();

    // Send message to peer with ID '0' using websockets
    server.send_reliable(0, "hello!".as_bytes().to_vec());
    
    // Send message to the same peer using webrtc data channel (UDP)
    server.send_unreliable(0, "hello again!".as_bytes().to_vec());

    // ...

}
```