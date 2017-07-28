extern crate shred;

use std::ops::{Index, IndexMut};
use std::any::TypeId;

const MAX_EVENTS: usize = 200;

struct RingBufferStorage<T> {
    data : Vec<T>,
    write_index : usize,
    max_size : usize
}

pub trait Event : Send + Sync + Clone + 'static {}

impl<T: Clone> RingBufferStorage<T> {
    pub fn new(size : usize) -> Self {
        RingBufferStorage {
            data: Vec::with_capacity(size),
            write_index : 0,
            max_size : size
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

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub struct ReaderId {
    t : TypeId,
    id : u32,
    read_index : usize
}

impl ReaderId {
    pub fn new(t : TypeId, id : u32) -> ReaderId {
        ReaderId {
            t : t,
            id : id,
            read_index : 0,
        }
    }
}

pub struct EventHandler {
    res : shred::Resources,
    next_reader_id: u32
}

impl EventHandler {
    pub fn new() -> EventHandler {
        EventHandler {
            res : shred::Resources::new(),
            next_reader_id: 0
        }
    }

    pub fn register<E : Event>(&mut self) {
        self.register_with_size::<E>(MAX_EVENTS);
    }

    pub fn register_with_size<E : Event>(&mut self, max_size : usize) {
        use shred::ResourceId;

        if self.res.has_value(ResourceId::new::<RingBufferStorage<E>>()) {
            return;
        }

        self.res.add(RingBufferStorage::<E>::new(max_size));
    }

    pub fn register_reader<E : Event>(&mut self) -> ReaderId {
        let reader_id = ReaderId::new(TypeId::of::<E>(), self.next_reader_id);
        self.next_reader_id += 1;
        reader_id
    }

    pub fn write<E : Event>(&mut self, events: &mut Vec<E>) {
        if events.len() == 0 {
            return;
        }
        let mut storage = self.res.fetch_mut::<RingBufferStorage<E>>(0);
        let mut write_index = storage.write_index;
        for e in events.drain(0..) {
            if write_index == storage.data.len() {
                storage.data.push(e);
            } else {
                storage.data[write_index] = e;
            }
            write_index += 1;
            if write_index >= storage.max_size {
                write_index = 0;
            }
        }
        storage.write_index = write_index;
    }

    pub fn write_single<E: Event>(&mut self, event: &E) {
        let mut storage = self.res.fetch_mut::<RingBufferStorage<E>>(0);
        let mut write_index = storage.write_index;
        if write_index == storage.data.len() {
            storage.data.push(event.clone());
        } else {
            storage.data[write_index] = event.clone();
        }
        write_index += 1;
        if write_index >= storage.max_size {
            write_index = 0;
        }
        storage.write_index = write_index;
    }

    pub fn read<E : Event + std::fmt::Debug>(&mut self, reader_id : &mut ReaderId) -> Vec<E> {
        let storage = self.res.fetch::<RingBufferStorage<E>>(0);
        let read_data = if storage.write_index >= reader_id.read_index {
            storage.data.get(reader_id.read_index..storage.write_index).unwrap().to_vec()
        } else {
            let mut d = storage.data.get(reader_id.read_index..storage.max_size).unwrap().to_vec();
            d.extend(storage.data.get(0..storage.write_index).unwrap().to_vec());
            d
        };
        reader_id.read_index = storage.write_index;
        read_data
    }

}