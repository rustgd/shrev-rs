//! Ring buffer implementation, that does immutable reads.

use std::marker::PhantomData;
use std::num::Wrapping;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender};

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

    /// A magic value (!0).
    /// This value gets set when calling `step` and we reached
    /// the end.
    fn magic(size: usize) -> Self {
        CircularIndex::new(!0, size)
    }

    fn is_magic(&self) -> bool {
        self.index == !0
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

#[derive(Derivative)]
#[derivative(Debug)]
struct Data<T> {
    #[derivative(Debug = "ignore")]
    data: Vec<T>,
    uninitialized: usize,
}

impl<T> Data<T> {
    fn new(size: usize) -> Self {
        let mut data = Data {
            data: vec![],
            uninitialized: 0,
        };

        unsafe {
            data.grow(0, size);
        }

        data
    }

    unsafe fn get(&self, index: usize) -> &T {
        self.data.get_unchecked(index)
    }

    unsafe fn put(&mut self, cursor: usize, elem: T) {
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

    /// Called when dropping the ring buffer.
    unsafe fn clean(&mut self, cursor: usize) {
        let mut cursor = CircularIndex::new(cursor, self.data.len());
        let end = cursor - 1;

        while let Some(i) = cursor.step(end) {
            if self.uninitialized > 0 {
                self.uninitialized -= 1;
            } else {
                ptr::drop_in_place(self.data.get_unchecked_mut(i) as *mut T);
            }
        }

        self.data.set_len(0);
    }

    #[cfg(test)]
    fn num_initialized(&self) -> usize {
        self.data.len() - self.uninitialized
    }
}

#[derive(Copy, Clone, Debug)]
struct Reader {
    generation: usize,
    last_index: usize,
}

impl Reader {
    fn set_inactive(&mut self) {
        self.last_index = !0;
    }

    fn active(&self) -> bool {
        self.last_index != !0
    }

    fn distance_from(&self, last: CircularIndex, current_gen: usize) -> usize {
        let this = CircularIndex {
            index: self.last_index,
            size: last.size,
        };

        match this - last.index {
            0 if self.generation == current_gen => last.size,
            x => x,
        }
    }

    fn needs_shift(&self, last_index: usize, current_gen: usize) -> bool {
        self.last_index > last_index
            || (self.last_index == last_index && self.generation != current_gen)
    }
}

/// The reader id is used by readers to tell the storage where the last read ended.
#[derive(Debug)]
pub struct ReaderId<T: 'static> {
    id: usize,
    marker: PhantomData<&'static [T]>,
    // stupid way to make this `Sync`,
    // never actually locked
    drop_notifier: Mutex<Sender<usize>>,
}

impl<T: 'static> Drop for ReaderId<T> {
    fn drop(&mut self) {
        let _ = self.drop_notifier.get_mut().send(self.id);
    }
}

#[derive(Debug)]
struct ReaderMeta {
    /// Free ids
    free: Vec<usize>,
    readers: Vec<Reader>,
}

impl ReaderMeta {
    fn alloc(&mut self, last_index: usize, generation: usize) -> usize {
        match self.free.pop() {
            Some(id) => {
                self.readers[id].last_index = last_index;
                self.readers[id].generation = generation;

                id
            }
            None => {
                let id = self.readers.len();
                self.readers.push(Reader {
                    generation,
                    last_index,
                });

                id
            }
        }
    }

    fn remove(&mut self, id: usize) {
        self.readers[id].set_inactive();
        self.free.push(id);
    }

    fn nearest_index(&self, last: CircularIndex, current_gen: usize) -> Option<&Reader> {
        self.readers
            .iter()
            .filter(|reader| reader.active())
            .min_by_key(|reader| reader.distance_from(last, current_gen))
    }

    fn shift(&mut self, last_index: usize, current_gen: usize, grow_by: usize) {
        for reader in &mut self.readers {
            let reader = reader as &mut Reader;
            if !reader.active() {
                continue;
            }

            if reader.needs_shift(last_index, current_gen) {
                reader.last_index += grow_by;
            }
        }
    }
}

/// Ring buffer, holding data of type `T`.
#[derive(Debug)]
pub struct RingBuffer<T> {
    available: usize,
    last_index: CircularIndex,
    data: Data<T>,
    // stupid way to make this `Sync`,
    // never actually locked
    free_rx: Mutex<Receiver<usize>>,
    // stupid way to make this `Sync`,
    // never actually locked
    free_tx: Mutex<Sender<usize>>,
    generation: Wrapping<usize>,
    nearest_reader: usize,
    meta: Mutex<ReaderMeta>, // TODO: consider `UnsafeCell`
}

impl<T: 'static> RingBuffer<T> {
    /// Create a new ring buffer with the given max size.
    pub fn new(size: usize) -> Self {
        assert!(size > 1);

        let (free_tx, free_rx) = mpsc::channel();
        let free_tx = Mutex::new(free_tx);
        let free_rx = Mutex::new(free_rx);

        RingBuffer {
            available: size,
            last_index: CircularIndex::at_end(size),
            data: Data::new(size),
            free_rx,
            free_tx,
            generation: Wrapping(0),
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
        I::IntoIter: ExactSizeIterator,
    {
        let iter = iter.into_iter();
        let len = iter.len();
        self.ensure_additional(len);
        for element in iter {
            unsafe {
                self.data.put(self.last_index + 1, element);
            }
            self.last_index += 1;
        }
        self.available -= len;
        self.generation += Wrapping(1);
    }

    /// Removes all elements from a `Vec` and pushes them to the ring buffer.
    pub fn drain_vec_write(&mut self, data: &mut Vec<T>) {
        self.iter_write(data.drain(..));
    }

    /// Ensures that `num` elements can be inserted.
    /// Does nothing if there's enough space, grows the buffer otherwise.
    #[inline(always)]
    pub fn ensure_additional(&mut self, num: usize) {
        if self.available >= num {
            return;
        }

        self.ensure_additional_slow(num);
    }

    #[inline(never)]
    fn ensure_additional_slow(&mut self, num: usize) {
        self.maintain();
        let meta = self.meta.get_mut();
        let left: usize = match meta.nearest_index(self.last_index, self.generation.0) {
            None => {
                self.available = self.last_index.size;

                return;
            }
            Some(reader) => {
                let left = reader.distance_from(self.last_index, self.generation.0);

                self.available = left;

                if left >= num {
                    return;
                } else {
                    left
                }
            }
        };
        let grow_by = num - left;
        let min_target_size = self.last_index.size + grow_by;

        // Make sure size' = 2^n * size
        let mut size = 2 * self.last_index.size;
        while size < min_target_size {
            size *= 2;
        }

        // Calculate adjusted growth
        let grow_by = size - self.last_index.size;

        // Insert the additional elements
        unsafe {
            self.data.grow(self.last_index + 1, grow_by);
        }
        self.last_index.size = size;

        meta.shift(self.last_index.index, self.generation.0, grow_by);
        self.available = grow_by + left
    }

    fn maintain(&mut self) {
        let meta = self.meta.get_mut();
        while let Ok(id) = self.free_rx.get_mut().try_recv() {
            meta.remove(id);
        }
    }

    /// Write a single data point into the ring buffer.
    pub fn single_write(&mut self, element: T) {
        use std::iter::once;

        self.iter_write(once(element));
    }

    /// Create a new reader id for this ring buffer.
    pub fn new_reader_id(&mut self) -> ReaderId<T> {
        self.maintain();
        let last_index = self.last_index.index;
        let generation = self.generation.0;
        let id = self.meta.get_mut().alloc(last_index, generation);

        ReaderId {
            id,
            marker: PhantomData,
            drop_notifier: Mutex::new(self.free_tx.get_mut().clone()),
        }
    }

    /// Read data from the ring buffer, starting where the last read ended, and up to where the last
    /// element was written.
    pub fn read(&self, reader_id: &mut ReaderId<T>) -> StorageIterator<T> {
        let (last_read_index, gen) = {
            let mut meta = self.meta.lock();

            let reader = &mut meta.readers[reader_id.id];
            let old = reader.last_index;
            reader.last_index = self.last_index.index;
            let old_gen = reader.generation;
            reader.generation = self.generation.0;

            (old, old_gen)
        };
        let mut index = CircularIndex::new(last_read_index, self.last_index.size);
        index += 1;
        if gen == self.generation.0 {
            // It is empty
            index = CircularIndex::magic(index.size);
        }

        let iter = StorageIterator {
            data: &self.data,
            end: self.last_index.index,
            index,
        };

        iter
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            self.data.clean(self.last_index + 1);
        }
    }
}

/// Iterator over a slice of data in `RingBufferStorage`.
#[derive(Debug)]
pub struct StorageIterator<'a, T: 'a> {
    data: &'a Data<T>,
    /// Inclusive end
    end: usize,
    index: CircularIndex,
}

impl<'a, T> Iterator for StorageIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.index
            .step(self.end)
            .map(|i| unsafe { self.data.get(i) })
    }

    // Needed to fulfill contract of `ExactSizeIterator`
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();

        (len, Some(len))
    }
}

impl<'a, T> ExactSizeIterator for StorageIterator<'a, T> {
    fn len(&self) -> usize {
        match self.index.is_magic() {
            true => 0,
            false => (CircularIndex::new(self.end, self.index.size) - self.index.index) + 1,
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
    fn test_size() {
        let mut buffer = RingBuffer::<i32>::new(4);

        buffer.single_write(55);

        let mut reader = buffer.new_reader_id();

        buffer.iter_write(0..16);
        assert_eq!(buffer.read(&mut reader).len(), 16);

        buffer.iter_write(0..6);
        assert_eq!(buffer.read(&mut reader).len(), 6);
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
        assert_eq!(buffer.data.num_initialized(), 0);
    }

    #[test]
    fn test_too_large_write() {
        let mut buffer = RingBuffer::<Test>::new(10);
        // Events just go off into the void if there's no reader registered.
        let _reader = buffer.new_reader_id();
        buffer.drain_vec_write(&mut events(15));
        assert_eq!(buffer.data.num_initialized(), 15);
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

    #[test]
    fn test_reader_reuse() {
        let mut buffer = RingBuffer::<Test>::new(3);
        {
            let _reader_id = buffer.new_reader_id();
        }
        let _reader_id = buffer.new_reader_id();
        assert_eq!(_reader_id.id, 0);
        assert_eq!(buffer.meta.get_mut().readers.len(), 1);
    }

    #[test]
    fn test_prevent_excess_growth() {
        let mut buffer = RingBuffer::<Test>::new(3);
        let mut reader_id = buffer.new_reader_id();
        println!("Initial buffer state: {:#?}", buffer);
        println!("--- first write ---");
        buffer.drain_vec_write(&mut events(2));
        println!("--- second write ---");
        buffer.drain_vec_write(&mut events(2));
        println!("--- writes complete ---");
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
        // After writing 4 more events the buffer should have no reason to grow beyond 6 (2 * 3).
        assert_eq!(buffer.data.num_initialized(), 6);
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
