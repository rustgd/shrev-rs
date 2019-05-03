extern crate shrev;

use shrev::EventChannel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

fn main() {
    let mut channel = EventChannel::new();

    let mut reader1 = channel.register_reader();
    let mut reader2 = channel.register_reader();

    channel.single_write(TestEvent { data: 1 });

    // Prints one event
    println!("reader1 read: {:#?}", channel.read(&mut reader1));
    channel.single_write(TestEvent { data: 32 });

    // Prints two events
    println!("reader2 read: {:#?}", channel.read(&mut reader2));
    // Prints no events
    println!("reader2 read: {:#?}", channel.read(&mut reader2));
}
