//! Ring buffer implementation, that does immutable reads.

use std::marker::PhantomData;
use std::num::Wrapping;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use parking_lot::Mutex;

#[derive(Clone, Copy, Debug)]
struct CircularIndex {
    index: usize,
    size: usize,
}

impl Add<usize> for CircularIndex {
    type Output = usize;

    fn add(self, rhs: usize) -> usize {
        (self.index + rhs) % self.size
    }
}

impl AddAssign<usize> for CircularIndex {
    fn add_assign(&mut self, rhs: usize) {
        self.index = *self + rhs;
    }
}

impl Sub<usize> for CircularIndex {
    type Output = usize;

    fn sub(self, rhs: usize) -> usize {
        (self.size - rhs + self.index) % self.size
    }
}

impl SubAssign<usize> for CircularIndex {
    fn sub_assign(&mut self, rhs: usize) {
        self.index = *self - rhs;
    }
}

trait InsertOverwrite<T> {
    fn write_or_push(&mut self, index: usize, elem: T);
}

impl<T> InsertOverwrite<T> for Vec<T> {
    fn write_or_push(&mut self, index: usize, elem: T) {
        if self.len() == index {
            self.push(elem);
        } else {
            self[index] = elem;
        }
    }
}

struct Reader {

}

/// The reader id is used by readers to tell the storage where the last read ended.
#[derive(Debug)]
pub struct ReaderId<T: 'static> {
    index: usize,
    marker: PhantomData<&'static [T]>,
    num_wraps: usize,
}

#[derive(Debug)]
struct ReaderMeta {

}

/// Ring buffer, holding data of type `T`.
#[derive(Debug)]
pub struct RingBuffer<T> {
    current_index: CircularIndex,
    data: Vec<T>,
    nearest_reader: usize,
    meta: Mutex<ReaderMeta>,
    /// If `current_index` == `nearest_reader` and `needs_growth` is true,
    /// a call to `grow_internal` is necessary.
    needs_growth: bool,
    num_wraps: Wrapping<usize>,
}

impl<T: 'static> RingBuffer<T> {
    /// Create a new ring buffer with the given max size.
    pub fn new(size: usize) -> Self {
        assert!(size > 1);

        RingBuffer {
            current_index: CircularIndex { index: 0, size },
            data: vec![],
            nearest_reader: !0,
            meta: Mutex::new(ReaderMeta {}),
            needs_growth: false,
            num_wraps: Wrapping(0),
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

    /// Removes all elements from a `Vec` and pushes them to the ring buffer.
    pub fn drain_vec_write(&mut self, data: &mut Vec<T>) {
        self.iter_write(data.drain(..));
    }

    /// Ensures that `num` elements can be inserted.
    /// Does nothing if there's enough space, grows the buffer otherwise.
    pub fn ensure_additional(&mut self, num: usize) {

    }

    /// Write a single data point into the ring buffer.
    pub fn single_write(&mut self, element: T) {
        self.data.write_or_push(self.current_index.index, element);

        self.current_index += 1;
        if self.current_index.index == 0 {
            self.num_wraps += Wrapping(1);
        }
    }

    /// Create a new reader id for this ring buffer.
    pub fn new_reader_id(&mut self) -> ReaderId<T> {
        ReaderId {
            index: self.current_index.index,
            marker: PhantomData,
            num_wraps: self.num_wraps.0,
        }
    }

    /// Read data from the ring buffer, starting where the last read ended, and up to where the last
    /// element was written.
    pub fn read(&self, reader_id: &mut ReaderId<T>) -> StorageIterator<T> {
        let iter = StorageIterator {
            data: &self.data,
            full: self.current_index.index == reader_id.index
                && self.num_wraps.0 != reader_id.num_wraps,
            end: self.current_index.index,
            index: reader_id.index,
            wrap: self.current_index.size,
        };

        reader_id.index = self.current_index.index;
        reader_id.num_wraps = self.num_wraps.0;

        iter
    }
}

/// Iterator over a slice of data in `RingBufferStorage`.
#[derive(Debug)]
pub struct StorageIterator<'a, T: 'a> {
    data: &'a [T],
    full: bool,
    /// Exclusive end
    end: usize,
    index: usize,
    wrap: usize,
}

impl<'a, T> Iterator for StorageIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        println!(
            "Index: {}, end: {}, len: {}, wrap: {}",
            self.index,
            self.end,
            self.data.len(),
            self.wrap
        );

        if self.full || self.index != self.end {
            self.full = false;
            self.index = self.index % self.wrap;
            let elem = Some(&self.data[self.index]);
            self.index += 1;

            elem
        } else {
            None
        }
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
    fn test_circular() {
        let mut buffer = RingBuffer::<i32>::new(4);

        buffer.single_write(55);

        let mut reader = buffer.new_reader_id();

        buffer.iter_write(0..4);
        assert_eq!(
            buffer.read(&mut reader).cloned().collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn test_empty_write() {
        let mut buffer = RingBuffer::<Test>::new(10);
        buffer.drain_vec_write(&mut vec![]);
        assert_eq!(buffer.data.len(), 0);
    }

    #[test]
    fn test_too_large_write() {
        let mut buffer = RingBuffer::<Test>::new(10);
        // Events just go off into the void if there's no reader registered.
        let _reader = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(15));
        assert_eq!(buffer.data.len(), 15);
    }

    #[test]
    fn test_empty_read() {
        let mut buffer = RingBuffer::<Test>::new(10);
        let mut reader_id = buffer.new_reader_id();
        let data = buffer.read(&mut reader_id);
        assert_eq!(Vec::<Test>::default(), data.cloned().collect::<Vec<_>>())
    }

    #[test]
    fn test_empty_read_write_before_id() {
        let mut buffer = RingBuffer::<Test>::new(10);
        buffer.drain_vec_write(&mut events(2));
        let mut reader_id = buffer.new_reader_id();
        let data = buffer.read(&mut reader_id);
        assert_eq!(Vec::<Test>::default(), data.cloned().collect::<Vec<_>>())
    }

    #[test]
    fn test_read() {
        let mut buffer = RingBuffer::<Test>::new(10);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(2));
        assert_eq!(
            vec![Test { id: 0 }, Test { id: 1 }],
            buffer.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        assert_eq!(
            Vec::<Test>::new(),
            buffer.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_write_overflow() {
        let mut buffer = RingBuffer::<Test>::new(3);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(4));
        let data = buffer.read(&mut reader_id);
        assert_eq!(
            vec![
                Test { id: 0 },
                Test { id: 1 },
                Test { id: 2 },
                Test { id: 3 },
            ],
            data.cloned().collect::<Vec<_>>()
        );
    }

    /// If you're getting a compilation error here this test has failed!
    #[test]
    fn test_send_sync() {
        trait SendSync: Send + Sync {
            fn is_send_sync() -> bool;
        }

        impl<T> SendSync for T
        where
            T: Send + Sync,
        {
            fn is_send_sync() -> bool {
                true
            }
        }

        assert!(RingBuffer::<Test>::is_send_sync());
        assert!(ReaderId::<Test>::is_send_sync());
    }

    //    #[test]
    //    fn test_reader_reuse() {
    //        let mut buffer = RingBuffer::<Test>::new(3);
    //        {
    //            let _reader_id = buffer.new_reader_id();
    //        }
    //        let _reader_id = buffer.new_reader_id();
    //        assert_eq!(buffer.reader_internal.len(), 1);
    //    }

    #[test]
    fn test_prevent_excess_growth() {
        let mut buffer = RingBuffer::<Test>::new(3);
        let mut reader_id = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(2));
        buffer.drain_vec_write(&mut events(2));
        // we wrote 0,1,0,1, if the buffer grew correctly we'll get all of these back.
        assert_eq!(
            vec![
                Test { id: 0 },
                Test { id: 1 },
                Test { id: 0 },
                Test { id: 1 },
            ],
            buffer.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );

        buffer.drain_vec_write(&mut events(4));
        // After writing 4 more events the buffer should have no reason to grow beyond four.
        assert_eq!(buffer.data.len(), 4);
        assert_eq!(
            vec![
                Test { id: 0 },
                Test { id: 1 },
                Test { id: 2 },
                Test { id: 3 },
            ],
            buffer.read(&mut reader_id).cloned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_write_slice() {
        let mut buffer = RingBuffer::<Test>::new(10);
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
