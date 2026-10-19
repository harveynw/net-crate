# Example clients

These serve as examples implementations of a client connecting to the server and performing the necessary signalling to start producing events. 

You will need a server process running in order to use them:

```sh
mkdir -p examples
cat > examples/echo.rs <<'RS'
use net::{Event, Server};
use std::{thread::sleep, time::Duration};

fn main() {
    let (mut server, mut events) = Server::new("127.0.0.1:3000");
    loop {
        for event in events.pop_all() {
            if let Event::Received(id, data) = event {
                server.send_reliable(id, data.clone());
                server.send_unreliable(id, data);
            }
        }
        sleep(Duration::from_millis(10));
    }
}
RS
cargo run --example echo
```
