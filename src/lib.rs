//! Event channel, pull based, that use a ringbuffer for internal
//! storage, to make it possible to do immutable reads.
//!
//! See examples directory for examples.

#![deny(missing_docs)]

pub use storage::ReaderId;
pub use storage::StorageIterator as EventIterator;

use storage::RingBufferStorage;

mod storage;

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

    /// Register a reader.
    ///
    /// To be able to read events, a reader id is required. This is because otherwise the channel
    /// wouldn't know where in the ringbuffer the reader has read to earlier. This information is
    /// stored in the reader id.
    pub fn register_reader(&mut self) -> ReaderId<E> {
        self.storage.new_reader_id()
    }

    /// Write a slice of events into storage
    pub fn slice_write(&mut self, events: &[E])
    where
        E: Clone,
    {
        self.storage.iter_write(events.into_iter().cloned());
    }

    /// Drain a vector of events into storage.
    pub fn drain_vec_write(&mut self, events: &mut Vec<E>) {
        self.storage.drain_vec_write(events);
    }

    /// Write a single event into storage.
    pub fn single_write(&mut self, event: E) {
        self.storage.single_write(event);
    }

    /// Read any events that have been written to storage since the readers last read.
    pub fn read(&self, reader_id: &mut ReaderId<E>) -> EventIterator<E> {
        self.storage.read(reader_id)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Test {
        pub id: u32,
    }

    #[test]
    fn test_read_write() {
        let mut channel = EventChannel::with_capacity(14);

        let mut reader_id = channel.register_reader();
        let mut reader_id_extra = channel.register_reader();

        channel.single_write(Test { id: 1 });
        assert_eq!(vec![Test { id: 1 }], channel.read(&mut reader_id).cloned().collect::<Vec<_>>());
        channel.single_write(Test { id: 2 });
        assert_eq!(vec![Test { id: 2 }], channel.read(&mut reader_id).cloned().collect::<Vec<_>>());

        assert_eq!(
            vec![Test { id: 1 }, Test { id: 2 }],
            channel.read(&mut reader_id_extra).cloned().collect::<Vec<_>>()
        );

        channel.single_write(Test { id: 3 });
        assert_eq!(vec![Test { id: 3 }], channel.read(&mut reader_id).cloned().collect::<Vec<_>>());
        assert_eq!(vec![Test { id: 3 }], channel.read(&mut reader_id_extra).cloned().collect::<Vec<_>>());
    }
}
