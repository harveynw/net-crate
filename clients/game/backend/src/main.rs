use log::{info, warn};
use net::{EventQueue, Server};
use std::{collections::HashSet, time::{Duration, Instant}};

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
    let mut active = HashSet::new();
    let mut angle: f32 = 0.0;

    loop {
        // Save current time
        let frame_start = Instant::now();
        
        // Update list of active players
        for event in queue.pop_all() {
            match event {
                net::Event::Open(id) => { let _ = active.insert(id); },
                net::Event::Closed(id) => { let _ = active.remove(&id); },
                net::Event::Received(id, bytes) => info!("From {}, got {:?}", id, bytes),
            };
        }
        
        // Calculate new angle
        angle += 0.01;

        // Broadcast new angle
        for id in &active {
            server.send_unreliable(*id, message::serialize(message::ServerMessage::Angle(angle)));
        }
        
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
