#![allow(unused)]

use shrev::*;

type EventChannel = shrev::EventChannel<i32>;

#[test]
fn naive_negative() {
    let mut channel = EventChannel::new();

    assert!(!channel.would_write());
}

#[test]
fn naive_positive() {
    let mut channel = EventChannel::new();
    let r = channel.register_reader();

    assert!(channel.would_write());
}

#[test]
fn all_dropped() {
    let mut channel = EventChannel::new();

    {
        let r1 = channel.register_reader();
        let r2 = channel.register_reader();
        let r3 = channel.register_reader();
    }

    assert!(!channel.would_write());
}

#[test]
fn recreated() {
    let mut channel = EventChannel::new();

    {
        let r1 = channel.register_reader();
        let r2 = channel.register_reader();
        let r3 = channel.register_reader();
    }
    let r1 = channel.register_reader();
    let r2 = channel.register_reader();

    assert!(channel.would_write());
}
