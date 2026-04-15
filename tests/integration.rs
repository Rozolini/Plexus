use std::ptr::NonNull;

use plexus::{SharedArena, SharedChannel, SharedEndpoint};

#[test]
fn arena_allocation_and_channel_roundtrip() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test controls buffer lifetime and mutability.
    let mut arena =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };

    let value = 42_u64;
    let handle = arena.allocate_value(value).expect("allocate");
    arena.commit();

    let channel = SharedChannel::<u64>::new(128).expect("channel");
    let (producer, consumer) = channel.split();
    producer.send(handle).expect("send");
    let received = consumer.try_recv().expect("recv");

    let read_back = arena.read_copy(received).expect("read");
    assert_eq!(read_back, 42);
}

#[test]
fn shared_endpoint_copy_roundtrip() {
    let mut backing = vec![0_u8; 8192];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test controls arena backing bytes.
    let arena =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };

    let endpoint = SharedEndpoint::<u64>::new(arena, 128).expect("endpoint");
    endpoint.send_copy(123).expect("send");
    let value = endpoint.try_recv_copy().expect("recv");
    assert_eq!(value, 123);

    let snapshot = endpoint.metrics().snapshot();
    assert_eq!(snapshot.enqueue_ok, 1);
    assert_eq!(snapshot.dequeue_ok, 1);
}
