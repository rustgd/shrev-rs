extern crate shrev;

use shrev::EventHandler;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEvent2 {
    data: u32,
}

fn main() {
    let mut event_handler = EventHandler::new();
    event_handler.register::<TestEvent>();
    event_handler.register::<TestEvent2>();
    let mut reader_id_2 = event_handler.register_reader::<TestEvent2>().unwrap();

    event_handler
        .write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }])
        .expect("");
    event_handler
        .write(&mut vec![TestEvent2 { data: 3 }, TestEvent2 { data: 4 }])
        .expect("");

    let mut reader_id_1 = event_handler.register_reader::<TestEvent>().unwrap();

    // Should have data, as reader was created before the first write
    assert_eq!(
        Ok([TestEvent2 { data: 3 }, TestEvent2 { data: 4 }].to_vec()),
        event_handler.read::<TestEvent2>(&mut reader_id_2)
    );

    // Should be empty, because no write was done after previous read
    assert_eq!(
        Ok([].to_vec()),
        event_handler.read::<TestEvent2>(&mut reader_id_2)
    );

    // Should be empty, because reader was created after the write
    assert_eq!(
        Ok([].to_vec()),
        event_handler.read::<TestEvent>(&mut reader_id_1)
    );

    event_handler
        .write(&mut vec![TestEvent2 { data: 8 }, TestEvent2 { data: 9 }])
        .expect("");

    // Should have data, as a second write was done
    assert_eq!(
        Ok([TestEvent2 { data: 8 }, TestEvent2 { data: 9 }].to_vec()),
        event_handler.read::<TestEvent2>(&mut reader_id_2)
    );
}
