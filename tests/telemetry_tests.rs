use std::ptr::NonNull;

use plexus::{SharedArena, SharedEndpoint};

#[test]
fn endpoint_updates_latency_and_depth_metrics() {
    let mut backing = vec![0_u8; 8192];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test controls lifetime of raw memory.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let endpoint = SharedEndpoint::<u64>::new(arena, 64).expect("endpoint");

    endpoint.send_copy(10).expect("send");
    endpoint.send_copy(20).expect("send");
    let _ = endpoint.try_recv_copy().expect("recv");
    let _ = endpoint.try_recv_copy().expect("recv");

    let snapshot = endpoint.metrics().snapshot();
    assert!(snapshot.send_latency_samples >= 2);
    assert!(snapshot.send_latency_avg_ns > 0);
    assert!(snapshot.queue_depth_high_watermark >= 1);
}
