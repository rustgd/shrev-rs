
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::cell::UnsafeCell;

const DEFAULT_CAP: usize = 5;

#[derive(Debug)]
pub struct ResizableBuffer<T> {
    list: Vec<Option<T>>,

    // Inner ring buffer
    ring_cap: usize,
    ring_len: usize,

    // Entire ring buffer
    cap: usize,
    len: usize,

    write_index: usize,
    read_index: usize,

    readers: HashMap<usize, UnsafeCell<usize>>,
    current_reader: usize,
}

#[derive(Debug)]
pub struct ReaderId<T> {
    // This id should be unique, you cannot have two readers
    // with the same id.
    id: usize,
    phantom: PhantomData<T>,
}

pub struct ReadData<'a, T: 'a> {
    buffer: &'a ResizableBuffer<T>,
    current: usize,
}

impl<T> ResizableBuffer<T> {
    pub fn new() -> Self {
        let mut buffer = Vec::with_capacity(DEFAULT_CAP); 
        buffer.extend((0..DEFAULT_CAP).map(|_| None));
        let len = buffer.len();
        ResizableBuffer {
            list: buffer,

            ring_cap: len,
            ring_len: 0,

            cap: len,
            len: 0,

            write_index: 0,
            read_index: 0,

            readers: HashMap::new(),
            current_reader: 0,
        }
    }

    pub fn reader(&mut self) -> ReaderId<T> {
        let reader = ReaderId {
            id: self.current_reader,
            phantom: PhantomData,
        };

        self.readers.insert(self.current_reader, UnsafeCell::new(self.write_index));
        self.current_reader += 1;
        reader
    }

    pub fn push(&mut self, value: T) {
        if self.ring_len == self.ring_cap ||
           self.ring_cap != self.cap
        {
            self.list.push(Some(value));
            self.len += 1;
            self.cap += 1;
            if self.write_index == 0 {
                self.ring_len = self.len;
                self.ring_cap = self.cap;
            }
        }
        else {
            self.list[self.write_index] = Some(value);
            self.write_index = (self.write_index + 1) % self.ring_cap;
            self.len += 1;
            self.ring_len += 1;
        }

        /*
        if let Some(start) = self.split_start {
            if self.write_index == start {
                let split_end = match self.split_end {
                    Some(end) => end,
                    None => {
                        self.split_end = Some(self.list.len());
                        self.list.len()
                    }
                };

                self.write_index = split_end;  
            }
        }

        if self.write_index == self.list.len() {
            self.list.push(value);
        } else {
            self.list[self.write_index] = value;
        }

        self.write_index += 1;

        if self.write_index == self.capacity &&
           self.split_start == None &&
           self.read_index > 0
        {
            self.write_index = 0;
        }
        */
    }

    pub fn read(&self, reader: &mut ReaderId<T>) -> ReadData<T> {
        let data = ReadData {
            buffer: self,
            current: self.read_index,
        };

        // This is safe as long as the reader id unique.
        unsafe { *self.readers.get(&reader.id).unwrap().get() = self.write_index; }
        data
    }

    /*
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let mut value = None;
        ::std::mem::swap(&mut value, &mut self.list[self.read_index]);

        self.len -= 1;
        self.ring_len -= 1;

        if self.ring_len == 0 && self.len != 0 {
            self.read_index.store(self.ring_cap, Ordering::SeqCst);
            //self.read_index = self.ring_cap;
            self.write_index = 0;
            self.ring_len = self.len;
            self.ring_cap = self.cap;
        }
        else {
            self.read_index.load(self.
            self.read_index = (self.read_index + 1) % self.ring_cap;
        }

        value
    }
    */

    /// Gets the lowest reader index.
    /// 
    /// This takes a mutable ring buffer since otherwise these could
    /// be getting modified while we read.
    fn lowest_index(&mut self) -> usize {
        let mut lowest = self.len;
        for index in self.readers.values() {
            let index = unsafe { *index.get() };
            if index < lowest {
                lowest = index
            }
        }
        lowest
    }

    pub fn print_readers(&mut self) {
        println!("readers: {{");
        for (&key, ref value) in &self.readers {
            println!("  {:?}: {:?}", key, unsafe { *value.get() });
        }
        println!("}}");
    }

    pub fn print(&mut self)
        where T: ::std::fmt::Debug,
    {
        println!("Buffer: {:#?}", *self);
        self.print_readers();
    }
}

