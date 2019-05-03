use shrev::*;

fn is_sync<T: Sync>() {}
fn is_send<T: Send>() {}

#[test]
fn event_channel_bounds() {
    is_send::<EventChannel<i32>>();
    is_sync::<EventChannel<i32>>();
}

#[test]
fn reader_id_bounds() {
    is_send::<ReaderId<i32>>();
    is_sync::<ReaderId<i32>>();
}

#[test]
fn event_iterator_bounds() {
    is_send::<EventIterator<'static, i32>>();
    is_sync::<EventIterator<'static, i32>>();
}
