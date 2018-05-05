//! Ring buffer implementation, that does immutable reads.

use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::ptr;

use parking_lot::Mutex;

#[derive(Clone, Copy, Debug)]
struct CircularIndex {
    index: usize,
    size: usize,
}

impl CircularIndex {
    fn new(index: usize, size: usize) -> Self {
        CircularIndex { index, size }
    }

    fn at_end(size: usize) -> Self {
        CircularIndex {
            index: size - 1,
            size,
        }
    }

    fn step(&mut self, inclusive_end: usize) -> Option<usize> {
        match self.index {
            x if x == !0 => None,
            x if x == inclusive_end => {
                let r = Some(x);
                self.index = !0;

                r
            }
            x => {
                let r = Some(x);
                *self += 1;

                r
            }
        }
    }
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

// TODO: custom drop logic
struct Data<T> {
    data: Vec<T>,
    uninitialized: usize,
}

impl<T> Data<T> {
    unsafe fn get(&self, index: usize) -> &T {
        self.data.get_unchecked(index)
    }

    unsafe fn insert(&mut self, cursor: usize, elem: T) {
        if self.uninitialized > 0 {
            // There is no element stored under `cursor`
            // -> do not drop anything!
            ptr::write(self.data.get_unchecked_mut(cursor) as *mut T, elem);
            self.uninitialized -= 1;
        } else {
            // We can safely drop this, it's initialized.
            *self.data.get_unchecked_mut(cursor) = elem;
        }
    }

    /// `cursor` is the first position that gets moved to the back,
    /// free memory will be created between `cursor - 1` and `cursor`.
    unsafe fn grow(&mut self, cursor: usize, by: usize) {
        assert!(by >= self.data.len());

        // Calculate how many elements we need to move
        let to_move = self.data.len() - cursor;

        // Reserve space and set the new length
        self.data.reserve_exact(by);
        let new = self.data.len() + by;
        self.data.set_len(new);

        // Move the elements after the cursor to the end of the buffer.
        // Since we grew the buffer at least by the old length,
        // the elements are non-overlapping.
        let src = self.data.as_ptr().offset(cursor as isize);
        let dst = self.data.as_mut_ptr().offset((cursor + by) as isize);
        ptr::copy_nonoverlapping(src, dst, to_move);

        self.uninitialized += by;
    }
}

#[derive(Copy, Clone, Debug)]
struct Reader {
    last_index: usize,
}

/// The reader id is used by readers to tell the storage where the last read ended.
#[derive(Debug)]
pub struct ReaderId<T: 'static> {
    id: usize,
    marker: PhantomData<&'static [T]>,
}

#[derive(Debug)]
struct ReaderMeta {
    /// Free ids
    free: Vec<usize>,
    readers: Vec<Reader>,
}

impl ReaderMeta {
    fn nearest_index(&self, current: CircularIndex) -> Option<CircularIndex> {
        self.readers
            .iter()
            .filter(|reader| reader.last_index != !0)
            .map(|r| r.last_index)
            .map(|index| CircularIndex {
                index,
                size: current.size,
            })
            .min_by_key(|&index| {
                index - current.index
            })
    }
}

/// Ring buffer, holding data of type `T`.
#[derive(Debug)]
pub struct RingBuffer<T> {
    last_index: CircularIndex,
    data: Vec<T>,
    nearest_reader: usize,
    meta: Mutex<ReaderMeta>,
}

impl<T: 'static> RingBuffer<T> {
    /// Create a new ring buffer with the given max size.
    pub fn new(size: usize) -> Self {
        assert!(size > 1);

        RingBuffer {
            last_index: CircularIndex::at_end(size),
            data: vec![],
            nearest_reader: !0,
            meta: Mutex::new(ReaderMeta {
                free: vec![],
                readers: vec![],
            }),
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
        let meta = self.meta.get_mut();
        let nearest = meta.nearest_index(CircularIndex::new(
            self.last_index + 1,
            self.last_index.size,
        ));

        let grow_by = match nearest {
            None => return,
            Some(index) => {
                let mut cursor = self.last_index.clone();
                cursor += 1;
                // If the cursor points to the nearest read index now, we still have one element
                // to write
                let left = (index - cursor) + 1;

                if left >= num {
                    return;
                } else {
                    num - left
                }
            }
        };
        // Make sure size' = 2^n * size
        let mut size = 2 * self.last_index.size;
        while size < grow_by {
            size *= 2;
        }

        // Insert the additional elements
    }

    /// Write a single data point into the ring buffer.
    pub fn single_write(&mut self, element: T) {
        self.ensure_additional(1);
        self.last_index += 1;
        self.data.write_or_push(self.last_index.index, element);
    }

    /// Create a new reader id for this ring buffer.
    pub fn new_reader_id(&mut self) -> ReaderId<T> {
        let meta = self.meta.get_mut();
        let last_index = self.last_index.index;
        let id = meta.free.pop().unwrap_or_else(|| {
            let id = meta.readers.len();
            meta.readers.push(Reader { last_index });

            id
        });

        ReaderId {
            id,
            marker: PhantomData,
        }
    }

    /// Read data from the ring buffer, starting where the last read ended, and up to where the last
    /// element was written.
    pub fn read(&self, reader_id: &mut ReaderId<T>) -> StorageIterator<T> {
        let iter = StorageIterator {
            data: &self.data,
            end: self.last_index.index,
            index: unimplemented!(),
        };

        //reader_id.index = self.current_index.index;
        //reader_id.num_wraps = self.num_wraps.0;

        iter
    }
}

/// Iterator over a slice of data in `RingBufferStorage`.
#[derive(Debug)]
pub struct StorageIterator<'a, T: 'a> {
    data: &'a [T],
    /// Inclusive end
    end: usize,
    index: CircularIndex,
}

impl<'a, T> Iterator for StorageIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        println!(
            "Index: {:?}, end: {}, len: {}",
            self.index,
            self.end,
            self.data.len(),
        );

        //        if self.index != self.end {
        //            //self.full = false;
        //            self.index = self.index % self.wrap;
        //            let elem = Some(&self.data[self.index]);
        //            self.index += 1;
        //
        //            elem
        //        } else {
        //            None
        //        }
        self.index.step(self.end).map(|i| &self.data[i])
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
