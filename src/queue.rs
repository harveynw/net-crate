use std::{collections::VecDeque, sync::{Arc, Mutex}};

use crate::event::Event;

/// An event queue, for receiving events that occur on the server.
/// 
/// Can be freely cloned, will point to the same underlying message buffer.
#[derive(Default, Clone)]
pub struct EventQueue {
    queue: Arc<Mutex< VecDeque<Event> >>,
}

impl EventQueue {
    /// Push an event onto the event queue
    pub(crate) fn push(&mut self, event: Event) {
        self.queue
            .lock()
            .expect("Lock should not be poisoned")
            .push_back(event);
    }

    /// Returns all events currently on the event queue, which will now be empty.
    pub fn pop_all(&mut self) -> Vec<Event> {
        self.queue
            .lock()
            .expect("Lock should not be poisoned")
            .drain(..)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::EventQueue;
    use super::Event;

    #[test]
    fn queue_works() {
        let mut q = EventQueue::default();

        // Put three in
        q.push(Event::Received(0, "Hello world!".as_bytes().to_vec()));
        q.push(Event::Received(1, "Hello world!!".as_bytes().to_vec()));
        q.push(Event::Received(2, "Hello world!!!".as_bytes().to_vec()));

        let out = q.pop_all();

        // Get three out
        assert!(out.len() == 3);

        // Now get nothing out
        assert!(q.pop_all().is_empty());
    }
}