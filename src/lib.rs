//! Event channel, pull based, that use a ringbuffer for internal
//! storage, to make it possible to do immutable reads.
//!
//! See examples directory for examples.

#![warn(missing_docs)]

pub use crate::storage::{ReaderId, StorageIterator as EventIterator};

use crate::storage::RingBuffer;

mod storage;
mod util;

/// Marker trait for data to use with the EventChannel.
///
/// Has an implementation for all types where its bounds are satisfied.
pub trait Event: Send + Sync + 'static {}

impl<T> Event for T where T: Send + Sync + 'static {}

const DEFAULT_CAPACITY: usize = 64;

/// Event channel
#[derive(Debug)]
pub struct EventChannel<E> {
    storage: RingBuffer<E>,
}

impl<E> Default for EventChannel<E>
where
    E: Event,
{
    fn default() -> Self {
        EventChannel::with_capacity(DEFAULT_CAPACITY)
    }
}

impl<E> EventChannel<E>
where
    E: Event,
{
    /// Create a new EventChannel with a default size of 50
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new EventChannel with the given starting capacity.
    pub fn with_capacity(size: usize) -> Self {
        Self {
            storage: RingBuffer::new(size),
        }
    }

    /// Register a reader.
    ///
    /// To be able to read events, a reader id is required. This is because
    /// otherwise the channel wouldn't know where in the ringbuffer the
    /// reader has read to earlier. This information is stored in the reader
    /// id.
    pub fn register_reader(&mut self) -> ReaderId<E> {
        self.storage.new_reader_id()
    }

    /// Write a slice of events into storage
    #[deprecated(note = "please use `iter_write` instead")]
    pub fn slice_write(&mut self, events: &[E])
    where
        E: Clone,
    {
        self.storage.iter_write(events.into_iter().cloned());
    }

    /// Write an iterator of events into storage
    pub fn iter_write<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = E>,
        I::IntoIter: ExactSizeIterator,
    {
        self.storage.iter_write(iter);
    }

    /// Drain a vector of events into storage.
    pub fn drain_vec_write(&mut self, events: &mut Vec<E>) {
        self.storage.drain_vec_write(events);
    }

    /// Write a single event into storage.
    pub fn single_write(&mut self, event: E) {
        self.storage.single_write(event);
    }

    /// Read any events that have been written to storage since the last read with `reader_id`.
    ///
    /// Note that this will advance the position of the reader regardless of what you do with the
    /// iterator. In other words, calling `read` without iterating the result won't preserve the
    /// events returned. You need to iterate all the events as soon as you got them from this
    /// method. This behavior is equivalent to e.g. `Vec::drain`.
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
    fn test_grow() {
        let mut channel = EventChannel::with_capacity(10);

        let mut reader0 = channel.register_reader();
        let mut reader1 = channel.register_reader();

        channel.iter_write(vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let data = channel.read(&mut reader0).cloned().collect::<Vec<_>>();
        assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        channel.iter_write(vec![9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22]);

        let data = channel.read(&mut reader0).cloned().collect::<Vec<_>>();
        assert_eq!(
            data,
            vec![9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22]
        );

        for i in 23..10_000 {
            channel.single_write(i);
        }

        let data = channel.read(&mut reader1).cloned().collect::<Vec<_>>();
        assert_eq!(data, (1..10_000).collect::<Vec<_>>());
    }

    #[test]
    fn test_read_write() {
        let mut channel = EventChannel::with_capacity(14);

        let mut reader_id = channel.register_reader();
        let mut reader_id_extra = channel.register_reader();

        channel.single_write(Test { id: 1 });
        assert_eq!(
            vec![Test { id: 1 }],
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );
        channel.single_write(Test { id: 2 });
        assert_eq!(
            vec![Test { id: 2 }],
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        assert_eq!(
            vec![Test { id: 1 }, Test { id: 2 }],
            channel
                .read(&mut reader_id_extra)
                .cloned()
                .collect::<Vec<_>>()
        );

        channel.single_write(Test { id: 3 });
        assert_eq!(
            vec![Test { id: 3 }],
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );
        assert_eq!(
            vec![Test { id: 3 }],
            channel
                .read(&mut reader_id_extra)
                .cloned()
                .collect::<Vec<_>>()
        );
    }

    // There was previously a case where the tests worked but the example didn't, so
    // the example was added as a test case.
    #[test]
    fn test_example() {
        let mut channel = EventChannel::new();

        channel.drain_vec_write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }]);

        let mut reader_id = channel.register_reader();

        // Should be empty, because reader was created after the write
        assert_eq!(
            Vec::<TestEvent>::default(),
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        // Should have data, as a second write was done
        channel.single_write(TestEvent { data: 5 });

        assert_eq!(
            vec![TestEvent { data: 5 }],
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        // We can also just send in an iterator.
        channel.iter_write(
            [TestEvent { data: 8 }, TestEvent { data: 9 }]
                .iter()
                .cloned(),
        );

        assert_eq!(
            vec![TestEvent { data: 8 }, TestEvent { data: 9 }],
            channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TestEvent {
        data: u32,
    }
}
