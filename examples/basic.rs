extern crate shrev;

use shrev::{EventChannel, EventReadData, ResizableBuffer};

#[derive(Clone, PartialEq, Eq)]
pub struct TestEvent {
    data: u32,
}

impl ::std::fmt::Debug for TestEvent {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{:?}", self.data)
    }
}

fn main() {
    /*let mut channel = EventChannel::new();

    channel
        .drain_vec_write(&mut vec![TestEvent { data: 1 }, TestEvent { data: 2 }])
        .expect("");

    let mut reader_id = channel.register_reader();

    // Should be empty, because reader was created after the write
    match channel.read(&mut reader_id) {
        Ok(EventReadData::Data(data)) => assert_eq!(
            Vec::<TestEvent>::default(),
            data.cloned().collect::<Vec<_>>()
        ),
        _ => panic!(),
    }

    channel
        .slice_write(&mut [TestEvent { data: 8 }, TestEvent { data: 9 }])
        .expect("");

    // Should have data, as a second write was done
    match channel.lossy_read(&mut reader_id) {
        Ok(data) => assert_eq!(
            vec![TestEvent { data: 8 }, TestEvent { data: 9 }],
            data.cloned().collect::<Vec<_>>()
        ),
        _ => panic!(),
    }*/

    let mut counter = 0..10000;

    let mut resize = ResizableBuffer::new();
    let mut r1 = resize.reader();
    let mut r2 = resize.reader();
    let mut r3 = resize.reader();
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.print();
    resize.read(&mut r1);
    resize.read(&mut r2);
    resize.print();
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.push(TestEvent { data: counter.next().unwrap() });
    resize.print();
}
