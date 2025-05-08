use log::{info, warn};
use message::{deserialize, serialize, ServerMessage};
use net::{EventQueue, Server};
use std::{collections::HashMap, time::{Duration, Instant}};

mod message;

fn main() {
    env_logger::init();

    info!("Starting...");

    let (server , queue) = Server::new("127.0.0.1:3000");

    event_loop(server, queue);
}

fn event_loop(mut server: Server, mut queue: EventQueue) {
    // Target 60Hz: ~16.67ms per frame
    let target_frame_time = Duration::from_nanos(1_000_000_000 / 60);

    // State
    let mut players = HashMap::<u32, (f32, f32, f32)>::new();

    loop {
        // Save current time
        let frame_start = Instant::now();
        
        // Update list of active players
        for event in queue.pop_all() {
            match event {
                net::Event::Open(id) => { 
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(*k, serialize(ServerMessage::PlayerJoined(id))));
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(id, serialize(ServerMessage::PlayerJoined(*k))));
                    players.insert(id, (0.0, 0.0, 0.0)); 
                },
                net::Event::Closed(id) => {
                    players.remove(&id); 
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(*k, serialize(ServerMessage::PlayerLeft(id))));
                },
                net::Event::Received(id, bytes) => {
                    let message = deserialize(bytes);

                    #[allow(irrefutable_let_patterns)]
                    if let message::ClientMessage::Move(x, y, z) = message {
                        players.insert(id, (x, y, z));
                    }
                }
            };
        }

        // Broadcast locations
        players
            .keys()
            .for_each(|k| server.send_reliable(*k, serialize(ServerMessage::Update(players.clone()))));
        
        // Calculate elapsed time
        let elapsed = frame_start.elapsed();
        
        // Sleep for remaining time to hit target frame rate
        if elapsed < target_frame_time {
            let sleep_time = target_frame_time - elapsed;
            std::thread::sleep(sleep_time);
        } else {
            let lag = elapsed - target_frame_time;
            warn!("Server lagged by {} ms", lag.as_millis()); 
        }
    }
}
