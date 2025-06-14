use log::{info, warn};
use net::{EventQueue, Server};
use std::time::{Duration, Instant};

/*
    Example server:

    Listens on port 3000, prints out events.
*/

fn main() {
    env_logger::init();

    info!("Starting...");

    let (server , queue) = Server::new("127.0.0.1:3000");

    event_loop(server, queue);
}

fn event_loop(_server: Server, mut queue: EventQueue) {
    // Target 60Hz: ~16.67ms per frame
    let target_frame_time = Duration::from_nanos(1_000_000_000 / 60);

    loop {
        // Save current time
        let frame_start = Instant::now();
        
        for event in queue.pop_all() {
            println!("Event: {:?}", event);
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
