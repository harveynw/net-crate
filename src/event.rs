pub type Identifier = u32;

#[derive(Debug)]
pub enum Event {
    Open(Identifier),
    Closed(Identifier), // + reason
    Received(Identifier, Vec<u8>)
}