pub type Identifier = u64;

#[derive(Debug)]
pub enum Event {
    Open(Identifier),
    Closed(Identifier), // + reason
    Received(Identifier, Vec<u8>)
}