extern crate shrev;

use shrev::*;

#[derive(Clone, Debug)]
pub struct TestEvent {
    data : u32
}

impl Event for TestEvent {}

#[derive(Clone, Debug)]
pub struct TestEvent2 {
    data : u32
}

impl Event for TestEvent2 {}

fn main() {
    let mut event_handler = EventHandler::new();
    event_handler.register::<TestEvent>();
    event_handler.register::<TestEvent2>();

    let mut events = vec![TestEvent { data : 1}, TestEvent { data : 2}];
    event_handler.write(&mut events);
    let mut events = vec![TestEvent2 { data : 3}, TestEvent2 { data : 4}];
    event_handler.write(&mut events);

    let reader_id_2 = event_handler.register_reader::<TestEvent2>();

    println!("{:?}", event_handler.read::<TestEvent2>(reader_id_2));
    println!("{:?}", event_handler.read::<TestEvent2>(reader_id_2));

    let mut events = vec![TestEvent2 { data : 8}, TestEvent2 { data : 9}];
    event_handler.write(&mut events);
    println!("{:?}", event_handler.read::<TestEvent2>(reader_id_2));

}