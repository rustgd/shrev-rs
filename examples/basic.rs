extern crate shrev;

use shrev::EventChannel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

fn main() {
    let mut channel = EventChannel::new();

    channel
        .drain_vec_write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }]);

    let mut reader_id = channel.register_reader();

    // Should be empty, because reader was created after the write
    assert_eq!(
        Vec::<TestEvent>::default(),
        channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
    );
}
