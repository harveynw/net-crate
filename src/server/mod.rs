//! Websocket actor
//! - Listen for new connections
//! - Establish an ID
//! - Establish a webrtc actor/connection per websocket connection
//! - Kill the connection if asked
//! - Notify that connection is dead

use log::info;
use std::collections::HashMap;
use tokio::{net::{TcpListener, TcpStream}, runtime::Builder, select, sync::mpsc};

use connection::{ConnectionEvent, ConnectionHandle};
use crate::{event::{Event, Identifier}, queue::EventQueue};

mod webrtc;
mod connection;


enum ActorMessage {
    /*
        Messages from the event loop 
    */

    HandleConnectionEvent(Identifier, ConnectionEvent),
    HandleNewStream(TcpStream),

    /*
        Commands given to the actor 
    */

    Kill(Identifier),
    SendReliable(Identifier, Vec<u8>),
    SendUnreliable(Identifier, Vec<u8>),
    Broadcast(Vec<u8>)
}

/// Handle to a websocket + webrtc server, used for sending messages and killing active connections.
/// 
/// Can be freely cloned, will point to the same instance.
#[derive(Clone)]
pub struct Server {
    sender: mpsc::Sender<ActorMessage>
}

impl Server {
    /// Create new server, which will be spawned on a new OS thread.
    pub fn new(listen_addr: &str) -> (Self, EventQueue) {
        let listen_addr = listen_addr.to_string();

        // Create a message queue
        let queue = EventQueue::default();

        // Channel for the handle
        let (sender, mut receiver) = mpsc::channel(1024);

        // Channel for active connections to emit events
        let (sender_connection, mut receiver_connection) = mpsc::channel::<(Identifier, ConnectionEvent)>(1024);

        // Create an async runtime
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        // Create an OS thread, and then start the event loop on our runtime
        let queue_cloned = queue.clone();
        std::thread::spawn(move || {
            rt.block_on(async move {
                let mut actor = Actor::new(sender_connection, queue_cloned);

                let listener = TcpListener::bind(&listen_addr).await.expect("Should be able to bind to listen_addr");
                info!("Websockets server bound to {}", listen_addr);

                // Event loop : Poll actor messages, tcp server and connection events.
                loop {
                    select! {
                        handle = receiver.recv() => {
                            match handle {
                                Some(message) => actor.handle_message(message),
                                None => break, // Kill actor and all connections
                            };
                        },
                        Ok((stream, _)) = listener.accept() => {
                            actor.handle_message(ActorMessage::HandleNewStream(stream));
                        },
                        Some((id, connection_event)) = receiver_connection.recv() => {
                            actor.handle_message(ActorMessage::HandleConnectionEvent(id, connection_event));
                        },
                        else => {}
                    }
                }
            })
        });

        (Server { sender }, queue)
    }

    /// Signal to kill a connection with a given identifier. 
    /// 
    /// The event queue should receive an Event::Closed(id) with the same identifier to confirm the action.
    pub fn kill(&mut self, id: Identifier) {
        self.sender.blocking_send(ActorMessage::Kill(id)).expect("Actor should be alive");
    }

    /// Send a message down a connection with the given identifier. Uses websockets as a reliable communication protocol.
    pub fn send_reliable(&mut self, id: Identifier, bytes: Vec<u8>) {
        self.sender.blocking_send(ActorMessage::SendReliable(id, bytes)).expect("Actor should be alive");
    }

    /// Send a message down a connection with the given identifier. Uses a webrtc datachannel over UDP as an unreliable communication protocol.
    pub fn send_unreliable(&mut self, id: Identifier, bytes: Vec<u8>) {
        self.sender.blocking_send(ActorMessage::SendUnreliable(id, bytes)).expect("Actor should be alive");
    }

    /// Broadcast a message reliably down all active connections.
    pub fn broadcast(&mut self, bytes: Vec<u8>) {
        self.sender.blocking_send(ActorMessage::Broadcast(bytes)).expect("Actor should be alive");
    }

}


struct Actor {
    // Hold ownership of handles to connection actors.
    connections: HashMap<Identifier, connection_state::Connection>,
    // Hold reference to the message queue, on which we can push incoming messages.
    queue: EventQueue,
    // Hold a sender to clone and pass to new connection actors, so they can emit events to us.
    connection_emit: mpsc::Sender<(Identifier, ConnectionEvent)>
}

impl Actor {
    pub fn new(connection_emit: mpsc::Sender<(Identifier, ConnectionEvent)>, queue: EventQueue) -> Self {
        Self {
            connections: HashMap::new(),
            queue,
            connection_emit
        }
    }

    pub fn handle_message(&mut self, message: ActorMessage) {
        match message {
            ActorMessage::Kill(id) => {
                info!("Received kill instruction for connection={}", id);
                self.connections.remove(&id);
            },
            ActorMessage::HandleConnectionEvent(id, connection_event) => {
                info!("Event registered: {:?}", connection_event);
                match connection_event {
                    ConnectionEvent::ConnectionEstablished => {
                        // Set to ready
                        self.connections.get_mut(&id).expect("Connection should be stored here").set_alive();
                        self.queue.push(Event::Open(id));
                    },
                    ConnectionEvent::ConnectionTerminated => {
                        // Kill connection actor by dropping its handle
                        self.connections.remove(&id).expect("Connection should be stored here");
                        self.queue.push(Event::Closed(id));
                    },
                    ConnectionEvent::MessageReceived(message) => {
                        // Push to queue
                        self.queue.push(Event::Received(id, message));
                    }
                }
            },
            ActorMessage::HandleNewStream(tcp_stream) => {
                // Assign new identifier
                let id = self.next_free_identifier();

                // Spawn actor
                let handle = ConnectionHandle::new(id, self.connection_emit.clone(), tcp_stream);

                // Store ownership of handle whilst it initialises
                self.connections.insert(id, connection_state::Connection::new(handle));
            },
            ActorMessage::SendReliable(to, bytes) => {
                let conn = self.connections.get_mut(&to).expect("Connection with id should be available.");

                conn.get_handle().send_reliable(bytes);
            },
            ActorMessage::SendUnreliable(to, bytes) => {
                let conn = self.connections.get_mut(&to).expect("Connection with id should be available.");

                conn.get_handle().send_unreliable(bytes);
            },
            ActorMessage::Broadcast(bytes) => {
                self.connections
                    .values_mut()
                    .filter(|conn| conn.is_alive())
                    .for_each(|conn| conn.get_handle().send_reliable(bytes.clone()));
            },
        }
    }

    /// Provides an unused identifier for fresh connections to use, assumes Identifier type won't overflow if incremented by one.
    fn next_free_identifier(&self) -> Identifier {
        self.connections.keys().max().map(|i| i + 1).unwrap_or(0)
    }
}


/// Maintains a 'liveness' invariant on a ConnectionHandle, panicking if it has not been set to 'alive'.
mod connection_state {
    use super::connection::ConnectionHandle;

    pub struct Connection {
        alive: bool,
        handle: ConnectionHandle,
    }

    impl Connection {
        pub fn new(handle: ConnectionHandle) -> Self { Self { alive: false, handle } }
        pub fn set_alive(&mut self) { self.alive = true; }
        pub fn is_alive(&self) -> bool { self.alive }

        pub fn get_handle(&mut self) -> &mut ConnectionHandle {
            if self.alive {
                &mut self.handle
            } else {
                panic!("Connection not tracked as live yet.")
            }
        }
    }
}
