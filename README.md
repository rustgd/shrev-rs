A pull based event handler, with events stored in a ring buffer,
 meant to be used as a resource in [`specs`](specs).
 
[specs]: https://github.com/slide-rs/specs

Example

```rust
extern crate shrev;

use shrev::EventHandler;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

fn main() {
    let mut event_handler = EventHandler::new();

    event_handler
        .write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }])
        .expect("");

    let mut reader_id = event_handler.register_reader();

    // Should be empty, because reader was created after the write
    match event_handler.read(&mut reader_id) {
        Ok(EventReadData::Data(data)) => {
            assert_eq!(Vec::<TestEvent>::default(), data.cloned().collect::<Vec<_>>())
        },
        _ => panic!(),
    }

    event_handler
        .write(&mut vec![TestEvent { data: 8 }, TestEvent { data: 9 }])
        .expect("");

    // Should have data, as a second write was done
    match event_handler.read(&mut reader_id) {
        Ok(EventReadData::Data(data)) => {
            assert_eq!(vec![TestEvent { data: 8 }, TestEvent { data: 9 }], data.cloned().collect::<Vec<_>>())
        },
        _ => panic!(),
    }
}
```