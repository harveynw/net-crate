//! Websocket actor
//! - Listen for new connections
//! - Establish an ID
//! - Establish a webrtc actor/connection per websocket connection
//! - Kill the connection if asked
//! - Notify that connection is dead

use bytes::Bytes;
use log::{info, warn};
use webrtc::RtcConfig;
use std::collections::HashMap;
use tokio::{net::{TcpListener, TcpStream}, runtime::Builder, select, sync::mpsc};

use connection::{ConnectionEvent, ConnectionHandle};
use connection_state::{ConnectingConnection, Connection};
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
    SendReliable(Identifier, Bytes),
    SendUnreliable(Identifier, Bytes),
    Broadcast(Bytes)
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
                // Create websocket server
                let listener = TcpListener::bind(&listen_addr).await.expect("Should be able to bind to listen_addr");
                info!("Websockets server bound to {}", listen_addr);

                let rtc_config = RtcConfig::new(listener.local_addr().expect("Listener should have an address").ip());
                let mut actor = Actor::new(sender_connection, queue_cloned, rtc_config);

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
    pub fn send_reliable(&mut self, id: Identifier, bytes: Bytes) {
        self.sender.blocking_send(ActorMessage::SendReliable(id, bytes)).expect("Actor should be alive");
    }

    /// Send a message down a connection with the given identifier. Uses a webrtc datachannel over UDP as an unreliable communication protocol.
    pub fn send_unreliable(&mut self, id: Identifier, bytes: Bytes) {
        self.sender.blocking_send(ActorMessage::SendUnreliable(id, bytes)).expect("Actor should be alive");
    }

    /// Broadcast a message reliably down all active connections.
    pub fn broadcast(&mut self, bytes: Bytes) {
        self.sender.blocking_send(ActorMessage::Broadcast(bytes)).expect("Actor should be alive");
    }

}


struct Actor {
    // Hold ownership of handles to connection actors.
    connections: HashMap<Identifier, Connection>,
    // Hold reference to the message queue, on which we can push incoming messages.
    queue: EventQueue,
    // Hold a sender to clone and pass to new connection actors, so they can emit events to us.
    connection_emit: mpsc::Sender<(Identifier, ConnectionEvent)>,
    // WebRTC settings cloned for each new connection
    rtc_config: RtcConfig
}

impl Actor {
    pub fn new(connection_emit: mpsc::Sender<(Identifier, ConnectionEvent)>, queue: EventQueue, rtc_config: RtcConfig) -> Self {
        Self {
            connections: HashMap::new(),
            queue,
            connection_emit,
            rtc_config,
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
                        match self.connections.remove(&id) {
                            Some(Connection::Connecting(connection)) => {
                                self.connections.insert(id, Connection::Ready(connection.into_ready()));
                                self.queue.push(Event::Open(id));
                            },
                            Some(Connection::Ready(_)) => unreachable!("Duplicate ConnectionEstablished for connection={id}"),
                            None => unreachable!("ConnectionEstablished for untracked connection={id}"),
                        }
                    },
                    ConnectionEvent::ConnectionTerminated => {
                        // Kill connection actor by dropping its handle
                        if self.connections.remove(&id).is_some() {
                            self.queue.push(Event::Closed(id));
                        }
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
                let handle = ConnectionHandle::new(id, self.connection_emit.clone(), tcp_stream, self.rtc_config.clone());

                // Store ownership of handle whilst it initialises
                self.connections.insert(id, Connection::Connecting(ConnectingConnection::new(handle)));
            },
            ActorMessage::SendReliable(to, bytes) => {
                match self.connections.get_mut(&to) {
                    Some(Connection::Ready(connection)) => connection.send_reliable(bytes),
                    Some(Connection::Connecting(_)) => warn!("Dropping reliable message: connection={to} is not ready"),
                    None => warn!("Dropping reliable message: connection={to} is not tracked"),
                }
            },
            ActorMessage::SendUnreliable(to, bytes) => {
                match self.connections.get_mut(&to) {
                    Some(Connection::Ready(connection)) => connection.send_unreliable(bytes),
                    Some(Connection::Connecting(_)) => warn!("Dropping unreliable message: connection={to} is not ready"),
                    None => warn!("Dropping unreliable message: connection={to} is not tracked"),
                }
            },
            ActorMessage::Broadcast(bytes) => {
                for connection in self.connections.values_mut() {
                    if let Connection::Ready(connection) = connection {
                        connection.send_reliable(bytes.clone());
                    }
                }
            },
        }
    }

    /// Provides an unused identifier for fresh connections to use, assumes Identifier type won't overflow if incremented by one.
    fn next_free_identifier(&self) -> Identifier {
        self.connections.keys().max().map(|i| i + 1).unwrap_or(0)
    }
}


/// Types for protecting connection methods before upgrade.
mod connection_state {
    use bytes::Bytes;
    use super::connection::ConnectionHandle;

    pub(super) enum Connection {
        Connecting(ConnectingConnection),
        Ready(ReadyConnection),
    }

    pub(super) struct ConnectingConnection {
        handle: ConnectionHandle,
    }

    /// Establishment has been observed; subsequent transport failures remain possible.
    pub(super) struct ReadyConnection {
        handle: ConnectionHandle,
    }

    impl ConnectingConnection {
        pub(super) fn new(handle: ConnectionHandle) -> Self { Self { handle } }

        /// Called when upgrade is safe, e.g. when this actor receives ConnectionEstablished.
        pub(super) fn into_ready(self) -> ReadyConnection { ReadyConnection { handle: self.handle } }
    }

    impl ReadyConnection {
        pub(super) fn send_reliable(&mut self, bytes: Bytes) { self.handle.send_reliable(bytes); }

        pub(super) fn send_unreliable(&mut self, bytes: Bytes) { self.handle.send_unreliable(bytes); }
    }
}
