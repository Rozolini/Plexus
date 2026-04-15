use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use plexus::SharedRing;

#[test]
#[ignore = "heavy stress test"]
fn stress_ring_million_messages_integrity() {
    let ring = Arc::new(SharedRing::with_capacity(1024).expect("ring"));
    let producer_ring = Arc::clone(&ring);
    let consumer_ring = Arc::clone(&ring);
    let total = 1_000_000_u64;

    let producer = thread::spawn(move || {
        for i in 0..total {
            loop {
                if producer_ring.try_push(i).is_ok() {
                    break;
                }
                std::hint::spin_loop();
            }
        }
    });

    let consumer = thread::spawn(move || {
        let mut expected = 0_u64;
        while expected < total {
            match consumer_ring.try_pop() {
                Ok(value) => {
                    assert_eq!(value, expected);
                    expected += 1;
                }
                Err(_) => std::hint::spin_loop(),
            }
        }
    });

    producer.join().expect("producer");
    consumer.join().expect("consumer");
}

#[test]
#[ignore = "heavy mpmc stress test"]
fn stress_ring_million_messages_mpmc_integrity() {
    let ring = Arc::new(SharedRing::with_capacity(4096).expect("ring"));
    let producers = 4_usize;
    let consumers = 4_usize;
    let total = 1_000_000_usize;
    let chunk = total / producers;
    let seen = Arc::new((0..total).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
    let consumed = Arc::new(AtomicUsize::new(0));

    let mut producer_threads = Vec::new();
    for producer_id in 0..producers {
        let ring = Arc::clone(&ring);
        producer_threads.push(thread::spawn(move || {
            let start = producer_id * chunk;
            let end = if producer_id == producers - 1 {
                total
            } else {
                start + chunk
            };
            for value in start..end {
                loop {
                    if ring.try_push(value).is_ok() {
                        break;
                    }
                    std::hint::spin_loop();
                }
            }
        }));
    }

    let mut consumer_threads = Vec::new();
    for _ in 0..consumers {
        let ring = Arc::clone(&ring);
        let seen = Arc::clone(&seen);
        let consumed = Arc::clone(&consumed);
        consumer_threads.push(thread::spawn(move || {
            loop {
                if consumed.load(Ordering::Acquire) >= total {
                    break;
                }
                match ring.try_pop() {
                    Ok(value) => {
                        let prev = seen[value].fetch_add(1, Ordering::AcqRel);
                        assert_eq!(prev, 0, "duplicate consumption for value {value}");
                        consumed.fetch_add(1, Ordering::AcqRel);
                    }
                    Err(_) => std::hint::spin_loop(),
                }
            }
        }));
    }

    for t in producer_threads {
        t.join().expect("producer");
    }
    for t in consumer_threads {
        t.join().expect("consumer");
    }

    assert_eq!(consumed.load(Ordering::Acquire), total);
    for (idx, count) in seen.iter().enumerate() {
        assert_eq!(
            count.load(Ordering::Acquire),
            1,
            "value {idx} was not consumed exactly once"
        );
    }
}
