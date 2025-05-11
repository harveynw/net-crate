use log::{info, warn};
use net::{EventQueue, Server};
use message::{deserialize, serialize, PlayerState, ServerMessage};
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

    // Track state
    let mut players = HashMap::<u32, PlayerState>::new();

    loop {
        // Save current time
        let frame_start = Instant::now();
        
        // Handle server events
        for event in queue.pop_all() {
            match event {
                net::Event::Open(id) => { 
                    // Inform existing players that we have a new player
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(*k, serialize(ServerMessage::PlayerJoined(id))));
                    // Inform new player of existing players
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(id, serialize(ServerMessage::PlayerJoined(*k))));
                    // Track new player
                    players.insert(id, PlayerState::default()); 
                },
                net::Event::Closed(id) => {
                    // Untrack player
                    players.remove(&id); 
                    // Inform remaining players that player has left
                    players
                        .keys()
                        .for_each(|k| server.send_reliable(*k, serialize(ServerMessage::PlayerLeft(id))));
                },
                net::Event::Received(id, bytes) => {
                    // Handle an incoming message from a player
                    let message = deserialize(bytes);

                    #[allow(irrefutable_let_patterns)]
                    if let message::ClientMessage::Update(state) = message {
                        players.insert(id, state);
                    }
                }
            };
        }

        // Broadcast locations, which can be unreliable
        players
            .keys()
            .for_each(|k| server.send_unreliable(*k, serialize(ServerMessage::Update(players.clone()))));
        
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
