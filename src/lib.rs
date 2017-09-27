//! Event handler, pull based, that uses shred to synchronize access, and ringbuffers for internal
//! storage, to make it possible to do immutable reads.
//!
//! See examples directory for examples.

#![deny(missing_docs)]

pub use storage::RBError as EventError;
pub use storage::ReadData as EventReadData;
pub use storage::ReaderId;
pub use storage::StorageIterator as EventIterator;

use storage::RingBufferStorage;

mod storage;

/// Marker trait for data to use with the EventHandler.
///
/// Has an implementation for all types where its bounds are satisfied.
pub trait Event: Send + Sync + 'static {}

impl<T> Event for T
where
    T: Send + Sync + 'static,
{
}

const DEFAULT_MAX_SIZE: usize = 200;

/// Event handler for managing many separate event types.
pub struct EventHandler<E> {
    storage: RingBufferStorage<E>,
}

impl<E> EventHandler<E>
where
    E: Event,
{
    /// Create a new EventHandler with a default size of 200
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_SIZE)
    }

    /// Create a new EventHandler with the given max size
    pub fn with_capacity(size: usize) -> Self {
        Self {
            storage: RingBufferStorage::new(size),
        }
    }

    /// Register a reader.
    ///
    /// To be able to read events, a reader id is required. This is because otherwise the handler
    /// wouldn't know where in the ringbuffer the reader has read to earlier. This information is
    /// stored in the reader id.
    pub fn register_reader(&mut self) -> ReaderId {
        self.storage.new_reader_id()
    }

    /// Write a number of events into its storage.
    pub fn write(&mut self, events: &mut Vec<E>) -> Result<(), EventError> {
        if events.len() == 0 {
            return Ok(());
        }

        self.storage.write(events)
    }

    /// Write a single event into storage.
    pub fn write_single(&mut self, event: E) {
        self.storage.write_single(event);
    }

    /// Read any events that have been written to storage since the readers last read.
    pub fn read(&self, reader_id: &mut ReaderId) -> Result<EventReadData<E>, EventError> {
        self.storage.read(reader_id)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::any::TypeId;

    #[derive(Debug, Clone, PartialEq)]
    struct Test {
        pub id: u32,
    }

    #[test]
    fn test_register_reader() {
        let mut handler = EventHandler::<Test>::with_capacity(14);
        let reader_id = handler.register_reader();
        assert_eq!(ReaderId::new(TypeId::of::<Test>(), 1, 0, 0), reader_id);
    }

    #[test]
    fn test_read_write() {
        let mut handler = EventHandler::with_capacity(14);

        let mut reader_id = handler.register_reader();
        let mut reader_id_extra = handler.register_reader();

        handler.write_single(Test { id: 1 });
        match handler.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 1 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }

        handler.write_single(Test { id: 2 });
        match handler.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 2 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
        match handler.read(&mut reader_id_extra) {
            Ok(EventReadData::Data(data)) => assert_eq!(
                vec![Test { id: 1 }, Test { id: 2 }],
                data.cloned().collect::<Vec<_>>()
            ),
            _ => panic!(),
        }

        handler.write_single(Test { id: 3 });
        match handler.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 3 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
        match handler.read(&mut reader_id_extra) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 3 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
    }
}
