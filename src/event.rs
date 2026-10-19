use bytes::Bytes;

/// Type for uniquely identifying a user 
pub type Identifier = u32;

#[derive(Debug)]
pub enum Event {
    /// Communication with user has begun successfully.
    Open(Identifier),
    Closed(Identifier), // + reason
    Received(Identifier, Bytes)
}
