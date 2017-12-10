//! Event channel, pull based, that use a ringbuffer for internal
//! storage, to make it possible to do immutable reads.
//!
//! See examples directory for examples.

//#![deny(missing_docs)]

pub use storage::RBError as EventError;
pub use storage::ReadData as EventReadData;
pub use storage::ReaderId;
pub use storage::StorageIterator as EventIterator;
pub use resizable::ResizableBuffer;

use std::any::TypeId;

use storage::RingBufferStorage;

mod storage;
mod resizable;

/// Marker trait for data to use with the EventChannel.
///
/// Has an implementation for all types where its bounds are satisfied.
pub trait Event: Send + Sync + 'static {}

impl<T> Event for T
where
    T: Send + Sync + 'static,
{
}

const DEFAULT_MAX_SIZE: usize = 200;

/// Event channel
pub struct EventChannel<E> {
    storage: RingBufferStorage<E>,
}

impl<E> EventChannel<E>
where
    E: Event,
{
    /// Create a new EventChannel with a default size of 200
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_SIZE)
    }

    /// Create a new EventChannel with the given max size.
    pub fn with_capacity(size: usize) -> Self {
        Self {
            storage: RingBufferStorage::new(size),
        }
    }

    /// Returns the maximum number of events that can be stored at once.
    pub fn max_size(&self) -> usize {
        self.storage.max_size()
    }

    /// Register a reader.
    ///
    /// To be able to read events, a reader id is required. This is because otherwise the channel
    /// wouldn't know where in the ringbuffer the reader has read to earlier. This information is
    /// stored in the reader id.
    pub fn register_reader(&self) -> ReaderId {
        self.storage.new_reader_id()
    }

    /// Write a slice of events into storage
    pub fn slice_write(&mut self, events: &[E]) -> Result<(), EventError>
    where
        E: Clone,
    {
        self.storage.iter_write(events.into_iter().cloned())
    }

    /// Drain a vector of events into storage.
    pub fn drain_vec_write(&mut self, events: &mut Vec<E>) -> Result<(), EventError> {
        self.storage.drain_vec_write(events)
    }

    /// Write a single event into storage.
    pub fn single_write(&mut self, event: E) {
        self.storage.single_write(event);
    }

    /// Read any events that have been written to storage since the readers last read.
    pub fn read(&self, reader_id: &mut ReaderId) -> Result<EventReadData<E>, EventError> {
        self.storage.read(reader_id)
    }

    /// Read events with loss if there is `Overflow`. Will only print an error message, and return
    /// what could be salvaged.
    pub fn lossy_read(&self, reader_id: &mut ReaderId) -> Result<EventIterator<E>, EventError> {
        self.read(reader_id).map(|read_data| match read_data {
            EventReadData::Data(data) => data,
            EventReadData::Overflow(data, overflow) => {
                eprintln!(
                    "EventChannel/{:?} overflowed, {} events missed.",
                    TypeId::of::<E>(),
                    overflow
                );
                data
            }
        })
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
        let channel = EventChannel::<Test>::with_capacity(14);
        let reader_id = channel.register_reader();
        assert_eq!(ReaderId::new(TypeId::of::<Test>(), 0, 0), reader_id);
    }

    #[test]
    fn test_read_write() {
        let mut channel = EventChannel::with_capacity(14);

        let mut reader_id = channel.register_reader();
        let mut reader_id_extra = channel.register_reader();

        channel.single_write(Test { id: 1 });
        match channel.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 1 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }

        channel.single_write(Test { id: 2 });
        match channel.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 2 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
        match channel.read(&mut reader_id_extra) {
            Ok(EventReadData::Data(data)) => assert_eq!(
                vec![Test { id: 1 }, Test { id: 2 }],
                data.cloned().collect::<Vec<_>>()
            ),
            _ => panic!(),
        }

        channel.single_write(Test { id: 3 });
        match channel.read(&mut reader_id) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 3 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
        match channel.read(&mut reader_id_extra) {
            Ok(EventReadData::Data(data)) => {
                assert_eq!(vec![Test { id: 3 }], data.cloned().collect::<Vec<_>>())
            }
            _ => panic!(),
        }
    }
}
