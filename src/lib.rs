extern crate shred;

use std::fmt::Debug;

mod storage;

use storage::{RingBufferStorage, ReaderId, RBError};

pub trait Event : Send + Sync + Clone + 'static {}

const DEFAULT_MAX_SIZE: usize = 200;
pub struct EventHandler {
    res : shred::Resources,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError<E : Debug + Clone + PartialEq + Eq> {
    TooLargeWrite,
    LostData(Vec<E>, usize),
    InvalidReader,
    InvalidEventType
}

impl <E : Debug + Clone + PartialEq + Eq> Into<EventError<E>> for RBError<E> {
    fn into(self) -> EventError<E> {
        match self {
            RBError::TooLargeWrite => EventError::TooLargeWrite,
            RBError::InvalidReader => EventError::InvalidReader,
            RBError::LostData(retained, missed_num) => EventError::LostData(retained, missed_num)
        }
    }
}

impl EventHandler {
    pub fn new() -> EventHandler {
        EventHandler {
            res : shred::Resources::new()
        }
    }

    pub fn register<E : Event + Debug + Clone + PartialEq + Eq>(&mut self) {
        self.register_with_size::<E>(DEFAULT_MAX_SIZE);
    }

    pub fn register_with_size<E : Event + Debug + Clone + PartialEq + Eq>(&mut self, max_size : usize) {
        use shred::ResourceId;

        if self.res.has_value(ResourceId::new::<RingBufferStorage<E>>()) {
            return;
        }

        self.res.add(RingBufferStorage::<E>::new(max_size));
    }

    pub fn register_reader<E : Event + Debug + Clone + PartialEq + Eq>(&mut self) -> Result<ReaderId, EventError<E>> {
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => Ok(storage.new_reader_id()),
            None => Err(EventError::InvalidEventType)
        }
    }

    pub fn write<E : Event + Debug + Clone + PartialEq + Eq>(&mut self, events: &mut Vec<E>) -> Result<(), EventError<E>> {
        if events.len() == 0 {
            return Ok(());
        }
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => match storage.write(events) {
                Ok(_) => Ok(()),
                Err(err) => Err(err.into())
            },
            None => Err(EventError::InvalidEventType)
        }
    }

    pub fn write_single<E: Event + Debug + Clone + PartialEq + Eq>(&mut self, event: E) -> Result<(), EventError<E>> {
        match self.res.try_fetch_mut::<RingBufferStorage<E>>(0) {
            Some(ref mut storage) => {
                storage.write_single(event);
                Ok(())
            },
            None => Err(EventError::InvalidEventType)
        }
    }

    pub fn read<E : Event + Debug + Clone + PartialEq + Eq>(&self, reader_id : &mut ReaderId) -> Result<Vec<E>, EventError<E>> {
        match self.res.try_fetch::<RingBufferStorage<E>>(0) {
            Some(ref storage) => {
                match storage.read(reader_id) {
                    Ok(data) => Ok(data),
                    Err(err) => Err(err.into())
                }
            },
            None => Err(EventError::InvalidEventType)
        }
    }

}