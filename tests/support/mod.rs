use std::{collections::VecDeque, error::Error, net::SocketAddr, time::Duration};

use bytes::Bytes;
use net::{Event, EventQueue, Server};
use tokio::{
    task::spawn_blocking,
    time::{sleep, timeout},
};

mod client;
pub use client::Client;

pub type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TestServer {
    pub address: SocketAddr,
    server: Server,
    queue: EventQueue,
    pending: VecDeque<Event>,
}

impl TestServer {
    pub fn start() -> TestResult<Self> {
        // Server::new does not expose its bound address, so select an available
        // port before starting it. Client::connect retries until it is listening.
        let reservation = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = reservation.local_addr()?;
        drop(reservation);
        let (server, queue) = Server::new(&address.to_string());
        Ok(Self {
            address,
            server,
            queue,
            pending: VecDeque::new(),
        })
    }

    pub async fn next_event(&mut self) -> TestResult<Event> {
        timeout(IO_TIMEOUT, async {
            loop {
                if let Some(event) = self.pending.pop_front() {
                    return event;
                }
                self.pending.extend(self.queue.pop_all());
                if self.pending.is_empty() {
                    sleep(Duration::from_millis(2)).await;
                }
            }
        })
        .await
        .map_err(|_| "Timed out waiting for a server event".into())
    }

    pub async fn opened(&mut self) -> TestResult<u32> {
        match self.next_event().await? {
            Event::Open(id) => Ok(id),
            event => Err(format!("Expected Event::Open, got {event:?}").into()),
        }
    }

    pub async fn received(&mut self, expected_id: u32, expected: &[u8]) -> TestResult {
        match self.next_event().await? {
            Event::Received(id, bytes) => {
                assert_eq!(id, expected_id, "Message came from the wrong client");
                assert_eq!(bytes.as_ref(), expected);
                Ok(())
            }
            event => Err(format!("Expected Event::Received, got {event:?}").into()),
        }
    }

    pub async fn closed(&mut self, expected_id: u32) -> TestResult {
        match self.next_event().await? {
            Event::Closed(id) => {
                assert_eq!(id, expected_id);
                Ok(())
            }
            event => Err(format!("Expected Event::Closed, got {event:?}").into()),
        }
    }

    pub async fn send_reliable(&self, id: u32, bytes: Bytes) -> TestResult {
        let mut server = self.server.clone();
        spawn_blocking(move || server.send_reliable(id, bytes)).await?;
        Ok(())
    }

    pub async fn send_unreliable(&self, id: u32, bytes: Bytes) -> TestResult {
        let mut server = self.server.clone();
        spawn_blocking(move || server.send_unreliable(id, bytes)).await?;
        Ok(())
    }

    pub async fn broadcast(&self, bytes: Bytes) -> TestResult {
        let mut server = self.server.clone();
        spawn_blocking(move || server.broadcast(bytes)).await?;
        Ok(())
    }
}
