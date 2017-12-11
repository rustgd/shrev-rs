//! Ring buffer implementation, that does immutable reads.

use std::marker;
use std::ops::{Index, IndexMut};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

/// The reader id is used by readers to tell the storage where the last read ended.
#[derive(Hash, PartialEq, Clone, Debug)]
pub struct ReaderId<T> {
    reader_id: usize,
    buffer_id: usize,
    m: marker::PhantomData<T>,
}

impl<T> ReaderId<T> {
    /// Create a new reader id
    pub fn new(reader_id: usize, buffer_id: usize) -> ReaderId<T> {
        ReaderId {
            reader_id,
            buffer_id,
            m: marker::PhantomData,
        }
    }
}

/// This static value helps assign unique ids to every storage which are then propagated to
/// registered reader IDs, preventing reader IDs from being used with the wrong storage.
/// It's very important to prevent this because otherwise the unsafe code used for reading
/// could cause memory corruption.
static RING_BUFFER_ID: AtomicUsize = ATOMIC_USIZE_INIT;

/// Ring buffer, holding data of type `T`
#[derive(Debug)]
pub struct RingBufferStorage<T> {
    pub(crate) data: Vec<T>,
    buffer_id: usize,
    reader_written: Vec<usize>,
    reader_indices: Vec<usize>,
    write_index: usize,
    written: usize,
    reset_written: usize,
}

impl<T: 'static> RingBufferStorage<T> {
    /// Create a new ring buffer with the given max size.
    pub fn new(size: usize) -> Self {
        RingBufferStorage {
            data: Vec::with_capacity(size),
            buffer_id: RING_BUFFER_ID.fetch_add(1, Ordering::Relaxed),
            reader_written: Vec::new(),
            reader_indices: Vec::new(),
            write_index: 0,
            written: 0,
            reset_written: size * 1000,
        }
    }

    /// Iterates over all elements of `iter` and pushes them to the buffer.
    pub fn iter_write<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for d in iter {
            self.single_write(d);
        }
    }

    /// Removes all elements from a `Vec` and pushes them to the ringbuffer.
    pub fn drain_vec_write(&mut self, data: &mut Vec<T>) {
        self.iter_write(data.drain(..));
    }

    /// Write a single data point into the ringbuffer.
    pub fn single_write(&mut self, data: T) {
        self.written += 1;
        let need_growth = self.reader_written.iter().any(|&written| {
            let num_written = if self.written < written {
                self.written + (self.reset_written - written)
            } else {
                self.written - written
            };
            num_written > self.data.len()
        });
        if need_growth || self.data.len() == 0 {
            self.data.insert(self.write_index, data);
            for i in 0..self.reader_indices.len() {
                if self.reader_indices[i] > self.write_index {
                    self.reader_indices[i] += 1;
                }
            }
        } else {
            if self.write_index >= self.data.len() {
                self.write_index = 0;
            }
            self.data[self.write_index] = data;
        }
        self.write_index += 1;
        if self.write_index >= self.data.len() && !need_growth {
            self.write_index = 0;
        }
        if self.written > self.reset_written {
            self.written = 0;
        }
    }

    /// Create a new reader id for this ringbuffer.
    pub fn new_reader_id(&mut self) -> ReaderId<T> {
        let new_id = self.reader_written.len();
        self.reader_written.push(self.written);
        self.reader_indices.push(self.write_index);
        ReaderId::new(new_id, self.buffer_id)
    }

    /// Read data from the ringbuffer, starting where the last read ended, and up to where the last
    /// data was written.
    pub fn read(&self, reader_id: &mut ReaderId<T>) -> StorageIterator<T> {
        if reader_id.buffer_id != self.buffer_id {
            panic!("ReaderID used with an event buffer it's not registered to.  Not permitted!");
        }

        let read_index = self.reader_indices[reader_id.reader_id];

        // Update the reader indice inside the storage.  This is safe because the only time this
        // value can be updated is when there is both a mutable reference to the reader ID
        // and an immutable reference to the storage.  We also guaranteed above that this reader id
        // was created by this storage.
        unsafe {
            let pointer: &usize = &self.reader_written[reader_id.reader_id];
            let pointer: *const usize = pointer as *const usize;
            let pointer: *mut usize = pointer as *mut usize;
            *pointer = self.written;
            let pointer: &usize = &self.reader_indices[reader_id.reader_id] ;
            let pointer: *const usize = pointer as *const usize;
            let pointer: *mut usize = pointer as *mut usize;
            *pointer = self.write_index;
        }
        StorageIterator {
            storage: &self,
            current: read_index,
            end: self.write_index,
        }
    }
}

/// Iterator over a slice of data in `RingBufferStorage`.
#[derive(Debug)]
pub struct StorageIterator<'a, T: 'a> {
    storage: &'a RingBufferStorage<T>,
    current: usize,
    end: usize,
}

impl<'a, T> Iterator for StorageIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.current == self.end {
            None
        } else {
            if self.current == self.storage.data.len() && self.end != self.storage.data.len() {
                self.current = 0;
            }
            let item = &self.storage[self.current];
            self.current += 1;
            if self.current == self.storage.data.len() && self.end != self.storage.data.len() {
                self.current = 0;
            }
            Some(item)
        }
    }
}

impl<T> Index<usize> for RingBufferStorage<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T> IndexMut<usize> for RingBufferStorage<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Test {
        pub id: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Test2 {
        pub id: u32,
    }

    #[test]
    fn test_empty_write() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        buffer.drain_vec_write(&mut vec![]);
        assert_eq!(buffer.data.len(), 0);
    }

    #[test]
    fn test_too_large_write() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        // Events just go off into the void if there's no reader registered.
        let _reader = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(15));
        assert_eq!(buffer.data.len(), 15);
    }

    #[test]
    fn test_empty_read() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        let mut reader_id = buffer.new_reader_id();
        let data = buffer.read(&mut reader_id);
        assert_eq!(Vec::<Test>::default(), data.cloned().collect::<Vec<_>>())

    }

    #[test]
    fn test_empty_read_write_before_id() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        buffer.drain_vec_write(&mut events(2));
        let mut reader_id = buffer.new_reader_id();
        let data = buffer.read(&mut reader_id);
        assert_eq!(Vec::<Test>::default(), data.cloned().collect::<Vec<_>>())
    }

    #[test]
    fn test_read() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(2));
        let data = buffer.read(&mut reader_id);
        assert_eq!(
            vec![Test { id: 0 }, Test { id: 1 }],
            data.cloned().collect::<Vec<_>>()
        )
    }

    #[test]
    fn test_write_overflow() {
        let mut buffer = RingBufferStorage::<Test>::new(3);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(4));
        let data = buffer.read(&mut reader_id);
        assert_eq!(
            vec![Test { id: 0 }, Test { id: 1 }, Test { id: 2 }, Test { id: 3 }],
            data.cloned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_prevent_excess_growth() {
        let mut buffer = RingBufferStorage::<Test>::new(3);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(2));
        buffer.drain_vec_write(&mut events(2));
        // we wrote 0,1,0,1, if the buffer grew correctly we'll get all of these back.
        assert_eq!(
            vec![Test { id: 0 }, Test { id: 1 }, Test { id: 0 }, Test { id: 1 }],
            buffer.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        buffer.drain_vec_write(&mut events(2));
        buffer.drain_vec_write(&mut events(2));
        // After writing 4 more events the buffer should have no reason to grow beyond four.
        assert_eq!(buffer.data.len(), 4);
    }

    #[test]
    fn test_write_slice() {
        let mut buffer = RingBufferStorage::<Test>::new(10);
        let mut reader_id = buffer.new_reader_id();
        buffer.iter_write(events(2));
        let data = buffer.read(&mut reader_id);
        assert_eq!(
            vec![Test { id: 0 }, Test { id: 1 }],
            data.cloned().collect::<Vec<_>>()
        );
    }

    fn events(n: u32) -> Vec<Test> {
        (0..n).map(|i| Test { id: i }).collect::<Vec<_>>()
    }
}
