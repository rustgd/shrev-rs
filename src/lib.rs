extern crate shred;

use std::ops::{Index, IndexMut};
use std::any::TypeId;
use std::collections::HashMap;

const MAX_EVENTS: usize = 200;

struct VecStorage<T> {
    data : Vec<T>
}

pub trait Event : Send + Sync + Clone + 'static {}

impl<T: Clone> VecStorage<T> {
    pub fn new(size : usize) -> Self {
        VecStorage { data: Vec::with_capacity(size) }
    }
}

impl<T> Index<usize> for VecStorage<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T> IndexMut<usize> for VecStorage<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub struct ReaderId(TypeId, u32);

pub struct EventHandler {
    res : shred::Resources,
    next_reader_id: u32
}

impl EventHandler {
    pub fn new() -> EventHandler {
        let mut handler = EventHandler {
            res : shred::Resources::new(),
            next_reader_id: 0
        };
        handler.res.add(HashMap::<ReaderId, usize>::default());
        handler.res.add(HashMap::<TypeId, usize>::default());
        handler
    }

    pub fn register<E : Event>(&mut self) {
        use shred::ResourceId;

        if self.res.has_value(ResourceId::new::<VecStorage<E>>()) {
            return;
        }

        self.res.add(VecStorage::<E>::new(MAX_EVENTS));

        let mut writers = self.res.fetch_mut::<HashMap<TypeId, usize>>(0);
        writers.insert(TypeId::of::<E>(), 0);
    }

    pub fn register_reader<E : Event>(&mut self) -> ReaderId {
        let reader_id = ReaderId(TypeId::of::<E>(), self.next_reader_id);
        let mut readers = self.res.fetch_mut::<HashMap<ReaderId, usize>>(0);
        self.next_reader_id += 1;
        readers.insert(reader_id, 0);
        reader_id
    }

    pub fn write<E : Event>(&mut self, events: &mut Vec<E>) {
        if events.len() == 0 {
            return;
        }
        let mut storage = self.res.fetch_mut::<VecStorage<E>>(0);
        let type_id = TypeId::of::<E>();
        let mut writers = self.res.fetch_mut::<HashMap<TypeId, usize>>(0);
        let write_index = writers.get_mut(&type_id).expect("Writer data not found!");
        let start_write_index = *write_index;
        for e in events.drain(0..) {
            if *write_index == storage.data.len() {
                storage.data.push(e);
            } else {
                storage.data[*write_index] = e;
            }
            *write_index += 1;
            if *write_index >= MAX_EVENTS {
                *write_index = 0;
            }
        }
        for (reader_id, reader_index) in self.res.fetch_mut::<HashMap<ReaderId, usize>>(0).iter_mut() {
            if reader_id.0 == type_id && start_write_index < *reader_index &&
                (*write_index >= *reader_index || *write_index < start_write_index) {
                *reader_index = *write_index;
            }
        }
    }

    pub fn write_single<E: Event>(&mut self, event: &E) {
        let mut storage = self.res.fetch_mut::<VecStorage<E>>(0);
        let type_id = TypeId::of::<E>();
        let mut writers = self.res.fetch_mut::<HashMap<TypeId, usize>>(0);
        let write_index = writers.get_mut(&type_id).expect("Writer data not found!");
        let start_write_index = *write_index;
        if *write_index == storage.data.len() {
            storage.data.push(event.clone());
        } else {
            storage.data[*write_index] = event.clone();
        }
        *write_index += 1;
        if *write_index >= MAX_EVENTS {
            *write_index = 0;
        }
        for (reader_id, reader_index) in self.res.fetch_mut::<HashMap<ReaderId, usize>>(0).iter_mut() {
            if reader_id.0 == type_id && start_write_index < *reader_index &&
                (*write_index >= *reader_index || *write_index < start_write_index) {
                *reader_index = *write_index;
            }
        }
    }

    pub fn clear<E : Event>(&mut self) {
        let mut storage = self.res.fetch_mut::<VecStorage<E>>(0);
        storage.data.clear();
        let type_id = TypeId::of::<E>();
        let mut writers = self.res.fetch_mut::<HashMap<TypeId, usize>>(0);
        let write_index = writers.get_mut(&type_id).expect("Writer data not found!");
        *write_index = 0;
        for (reader_id, reader_index) in self.res.fetch_mut::<HashMap<ReaderId, usize>>(0).iter_mut() {
            if reader_id.0 == type_id {
                *reader_index = 0;
            }
        }
    }

    pub fn read<E : Event + std::fmt::Debug>(&mut self, reader_id : ReaderId) -> Vec<E> {
        let storage = self.res.fetch_mut::<VecStorage<E>>(0);
        let mut readers = self.res.fetch_mut::<HashMap<ReaderId, usize>>(0);
        let read_index = readers.get_mut(&reader_id).expect("Reader data not found!");
        let mut writers = self.res.fetch_mut::<HashMap<TypeId, usize>>(0);
        let write_index = writers.get_mut(&TypeId::of::<E>()).expect("Writer data not found!");
        let read_data = if write_index >= read_index {
            storage.data.get(*read_index..*write_index).unwrap().to_vec()
        } else {
            let mut d = storage.data.get(*read_index..MAX_EVENTS).unwrap().to_vec();
            d.extend(storage.data.get(0..*write_index).unwrap().to_vec());
            d
        };
        *read_index = *write_index;
        read_data
    }

}