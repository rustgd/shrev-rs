//! Event handler, pull based, that uses shred to synchronize access, and ringbuffers for internal
//! storage, to make it possible to do immutable reads.
//!
//! See examples directory for examples.

#![deny(missing_docs)]

extern crate shred;

pub use storage::ReaderId;

use std::fmt::Debug;

use storage::{RBError, RingBufferStorage};

mod storage;

/// Marker trait for data to use with the EventHandler.
///
/// Has an implementation for all types where its bounds are satisfied.
pub trait Event: Send + Sync + Clone + 'static {}
impl<T> Event for T
where
    T: Send + Sync + Clone + 'static,
{
}

const DEFAULT_MAX_SIZE: usize = 200;

/// Event handler for managing many separate event types.
pub struct EventHandler {
    res: shred::Resources,
}

/// Possible errors returned by the EventHandler
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError<E: Debug + Clone + PartialEq> {
    /// If a writer tries to write more data than the max size of the ringbuffer, in a single call
    TooLargeWrite,
    /// If a reader is more than the entire ringbuffer behind in reading, this will be returned.
    /// Contains the data that could be salvaged
    LostData(Vec<E>, usize),
    /// If attempting to use a reader for a different data type than the storage contains.
    InvalidReader,
    /// If attempting to read/write events or register a reader for an event type that has not been
    /// registered.
    InvalidEventType,
}

impl<E: Debug + Clone + PartialEq> Into<EventError<E>> for RBError<E> {
    fn into(self) -> EventError<E> {
        match self {
            RBError::TooLargeWrite => EventError::TooLargeWrite,
            RBError::InvalidReader => EventError::InvalidReader,
            RBError::LostData(retained, missed_num) => EventError::LostData(retained, missed_num),
        }
    }
}

impl EventHandler {
    /// Create a new EventHandler
    pub fn new() -> EventHandler {
        EventHandler { res: shred::Resources::new() }
    }

    /// Register an event type.
    ///
    /// This will register the event type and allocate ringbuffer storage for the event type with
    /// a default max size of 200 events. Once the 200 mark is reached, and a reader has still not
    /// read the events, the buffer will overflow, and the reader will get an error on the next
    /// read.
    pub fn register<E: Event + Debug + Clone + PartialEq>(&mut self) {
        self.register_with_size::<E>(DEFAULT_MAX_SIZE);
    }

    /// Register an event type, with a given maximum number of events before overflowing.
    ///
    /// This will register the event type and allocate ringbuffer storage for the event type with
    /// the given size. Once that size mark is reached, and a reader has still not read the events,
    /// the buffer will overflow, and the reader will get an error on the next read.
    pub fn register_with_size<E: Event + Debug + Clone + PartialEq>(&mut self, max_size: usize) {
        use shred::ResourceId;

        if self.res.has_value(
            ResourceId::new::<RingBufferStorage<E>>(),
        )
        {
            return;
        }

        self.res.add(RingBufferStorage::<E>::new(max_size));
    }

    /// Register a reader of an event type.
    ///
    /// To be able to read events, a reader id is required. This is because otherwise the handler
    /// wouldn't know where in the ringbuffer the reader has read to earlier. This information is
    /// stored in the reader id.
    pub fn register_reader<E: Event + Debug + Clone + PartialEq>(
        &mut self,
    ) -> Result<ReaderId, EventError<E>> {
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => Ok(storage.new_reader_id()),
            None => Err(EventError::InvalidEventType),
        }
    }

    /// Write a number of events into its storage.
    pub fn write<E: Event + Debug + Clone + PartialEq>(
        &mut self,
        events: &mut Vec<E>,
    ) -> Result<(), EventError<E>> {
        if events.len() == 0 {
            return Ok(());
        }
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => {
                match storage.write(events) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.into()),
                }
            }
            None => Err(EventError::InvalidEventType),
        }
    }

    /// Write a single event into its storage.
    pub fn write_single<E: Event + Debug + Clone + PartialEq>(
        &mut self,
        event: E,
    ) -> Result<(), EventError<E>> {
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => {
                storage.write_single(event);
                Ok(())
            }
            None => Err(EventError::InvalidEventType),
        }
    }

    /// Read any events that have been written to storage since the readers last read.
    pub fn read<E: Event + Debug + Clone + PartialEq>(
        &self,
        reader_id: &mut ReaderId,
    ) -> Result<Vec<E>, EventError<E>> {
        match self.res.try_fetch::<RingBufferStorage<E>>(0) {
            Some(ref storage) => {
                match storage.read(reader_id) {
                    Ok(data) => Ok(data),
                    Err(err) => Err(err.into()),
                }
            }
            None => Err(EventError::InvalidEventType),
        }
    }
}

#[cfg(test)]
mod tests {

    use std::any::TypeId;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Test {
        pub id : u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Test2 {
        pub id : u32,
    }

    #[test]
    fn test_register() {
        let mut handler = EventHandler::new();
        handler.register::<Test>();
        handler.register::<Test2>();
    }

    #[test]
    fn test_register_with_size() {
        let mut handler = EventHandler::new();
        handler.register_with_size::<Test>(14);
        handler.register_with_size::<Test2>(14);
    }

    #[test]
    fn test_register_reader() {
        let mut handler = EventHandler::new();
        handler.register_with_size::<Test>(14);
        let reader_id = handler.register_reader::<Test>();
        assert_eq!(Ok(ReaderId::new(TypeId::of::<Test>(), 1, 0, 0)), reader_id);
    }

    #[test]
    fn test_write_without_register() {
        let mut handler = EventHandler::new();
        assert_eq!(Err(EventError::InvalidEventType), handler.write_single(Test { id : 1 }));
    }

    #[test]
    fn test_register_reader_without_register() {
        let mut handler = EventHandler::new();
        assert_eq!(Err(EventError::InvalidEventType), handler.register_reader::<Test>());
    }

    #[test]
    fn test_read_write() {
        let mut handler = EventHandler::new();
        handler.register_with_size::<Test>(14);
        handler.register_with_size::<Test2>(14);

        let mut reader_id = handler.register_reader::<Test>().unwrap();
        let mut reader_id_extra = handler.register_reader::<Test>().unwrap();
        let mut reader_id_2 = handler.register_reader::<Test2>().unwrap();

        assert_eq!(Ok(()), handler.write_single(Test { id : 1 }));
        assert_eq!(Ok(vec![Test { id : 1 }]), handler.read(&mut reader_id));
        assert_eq!(Ok(Vec::<Test2>::default()), handler.read(&mut reader_id_2));

        assert_eq!(Ok(()), handler.write_single(Test { id : 2 }));
        assert_eq!(Ok(vec![Test { id : 2 }]), handler.read(&mut reader_id));
        assert_eq!(Ok(Vec::<Test2>::default()), handler.read(&mut reader_id_2));
        assert_eq!(Ok(vec![Test { id : 1 }, Test { id : 2 }]), handler.read(&mut reader_id_extra));

        assert_eq!(Ok(()), handler.write_single(Test { id : 3 }));
        assert_eq!(Ok(()), handler.write_single(Test2 { id : 6 }));
        assert_eq!(Ok(vec![Test { id : 3 }]), handler.read(&mut reader_id));
        assert_eq!(Ok(vec![Test2 { id : 6 }]), handler.read(&mut reader_id_2));
        assert_eq!(Ok(vec![Test { id : 3 }]), handler.read(&mut reader_id_extra));
    }
}