use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use plexus::{RingError, SharedRing};

#[test]
#[ignore = "long-running soak for nightly/heavy lanes"]
fn soak_endpoint_stability_over_configured_duration() {
    let soak_seconds = env::var("PLEXUS_SOAK_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    let duration = Duration::from_secs(soak_seconds);

    let ring = Arc::new(SharedRing::with_capacity(4096).expect("ring"));
    let stop = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(AtomicU64::new(0));
    let received = Arc::new(AtomicU64::new(0));

    let producer_ring = Arc::clone(&ring);
    let producer_stop = Arc::clone(&stop);
    let producer_sent = Arc::clone(&sent);
    let producer = thread::spawn(move || {
        let mut value = 0_u64;
        while !producer_stop.load(Ordering::Acquire) {
            match producer_ring.try_push(value) {
                Ok(()) => {
                    value = value.wrapping_add(1);
                    producer_sent.fetch_add(1, Ordering::AcqRel);
                }
                Err(RingError::Full) => std::hint::spin_loop(),
                Err(err) => panic!("unexpected push error during soak: {err:?}"),
            }
        }
    });

    let consumer_ring = Arc::clone(&ring);
    let consumer_stop = Arc::clone(&stop);
    let consumer_received = Arc::clone(&received);
    let consumer = thread::spawn(move || {
        while !consumer_stop.load(Ordering::Acquire) {
            match consumer_ring.try_pop() {
                Ok(_) => {
                    consumer_received.fetch_add(1, Ordering::AcqRel);
                }
                Err(RingError::Empty) => std::hint::spin_loop(),
                Err(err) => panic!("unexpected pop error during soak: {err:?}"),
            }
        }
    });

    // Keep the workload shape fixed (one hot producer/consumer pair) for nightly trend stability.
    thread::sleep(duration);
    stop.store(true, Ordering::Release);
    producer.join().expect("producer");
    consumer.join().expect("consumer");

    assert!(sent.load(Ordering::Acquire) > 0, "soak should send traffic");
    assert!(
        received.load(Ordering::Acquire) > 0,
        "soak should receive traffic"
    );
}
