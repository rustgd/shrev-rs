A pull based event channel, with events stored in a ring buffer,
meant to be used as a resource in [`specs`].
 
[`specs`]: https://github.com/slide-rs/specs

## Example usage

```rust
extern crate shrev;

use shrev::{EventChannel, EventReadData};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

fn main() {
    let mut channel = EventChannel::new();

    channel
        .drain_vec_write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }])
        .expect("");

    let mut reader_id = channel.register_reader();

    // Should be empty, because reader was created after the write
    match channel.read(&mut reader_id) {
        EventReadData::Data(data) => assert_eq!(
            Vec::<TestEvent>::default(),
            data.cloned().collect::<Vec<_>>()
        ),
        _ => panic!(),
    }

    channel
        .slice_write(&mut [TestEvent { data: 8 }, TestEvent { data: 9 }])
        .expect("");

    // Should have data, as a second write was done
    assert_eq!(
        vec![TestEvent { data: 8 }, TestEvent { data: 9 }],
        channel
            .lossy_read(&mut reader_id)
            .cloned()
            .collect::<Vec<_>>()
    );
}
```
