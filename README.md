A pull based event channel, with events stored in a ring buffer,
meant to be used as a resource in [`specs`].
 
[`specs`]: https://github.com/slide-rs/specs

## Example usage

```rust
extern crate shrev;

use shrev::EventChannel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

fn main() {
    let mut channel = EventChannel::new();

    channel.drain_vec_write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }]);

    let mut reader_id = channel.register_reader();

    // Should be empty, because reader was created after the write
    assert_eq!(
        Vec::<TestEvent>::default(),
        channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
    );

    // Should have data, as a second write was done
    channel.single_write(TestEvent { data: 5 });

    assert_eq!(
        vec![TestEvent { data: 5 }],
        channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
    );

    // We can also just send in an iterator.
    channel.iter_write(
        [TestEvent { data: 8 }, TestEvent { data: 9 }]
            .iter()
            .cloned(),
    );

    assert_eq!(
        vec![TestEvent { data: 8 }, TestEvent { data: 9 }],
        channel.read(&mut reader_id).cloned().collect::<Vec<_>>()
    );
}
```
