#![cfg(feature = "loom-tests")]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

struct Slot {
    sequence: AtomicUsize,
    value: UnsafeCell<usize>,
}

struct LoomRing {
    mask: usize,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    slots: Vec<Slot>,
}

impl LoomRing {
    fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 1 && capacity.is_power_of_two());
        let mut slots = Vec::with_capacity(capacity);
        for i in 0..capacity {
            slots.push(Slot {
                sequence: AtomicUsize::new(i),
                value: UnsafeCell::new(0),
            });
        }
        Self {
            mask: capacity - 1,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            slots,
        }
    }

    fn try_push(&self, value: usize) -> bool {
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let slot = &self.slots[tail & self.mask];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let diff = sequence as isize - tail as isize;
            if diff == 0 {
                if self
                    .tail
                    .compare_exchange_weak(tail, tail + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    slot.value.with_mut(|ptr| {
                        // SAFETY: slot ownership acquired by successful tail CAS.
                        unsafe { *ptr = value };
                    });
                    // Publish value after slot write; consumer observes via sequence acquire load.
                    slot.sequence.store(tail + 1, Ordering::Release);
                    return true;
                }
            } else if diff < 0 {
                return false;
            } else {
                thread::yield_now();
            }
        }
    }

    fn try_pop(&self) -> Option<usize> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let slot = &self.slots[head & self.mask];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let diff = sequence as isize - (head + 1) as isize;
            if diff == 0 {
                if self
                    .head
                    .compare_exchange_weak(head, head + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    let value = slot.value.with(|ptr| {
                        // SAFETY: slot ownership acquired by successful head CAS.
                        unsafe { *ptr }
                    });
                    // Mark slot reusable in next wrap-around cycle.
                    slot.sequence.store(head + self.capacity, Ordering::Release);
                    return Some(value);
                }
            } else if diff < 0 {
                return None;
            } else {
                thread::yield_now();
            }
        }
    }
}

#[test]
fn loom_ring_exactly_once_for_competing_producers() {
    loom::model(|| {
        let ring = Arc::new(LoomRing::with_capacity(4));
        let p1_ring = Arc::clone(&ring);
        let p2_ring = Arc::clone(&ring);
        let producer_a = thread::spawn(move || {
            while !p1_ring.try_push(10) {
                thread::yield_now();
            }
        });
        let producer_b = thread::spawn(move || {
            while !p2_ring.try_push(20) {
                thread::yield_now();
            }
        });

        producer_a.join().expect("join");
        producer_b.join().expect("join");

        let mut values = vec![
            ring.try_pop().expect("first value"),
            ring.try_pop().expect("second value"),
        ];
        values.sort_unstable();
        assert_eq!(values, vec![10, 20]);
    });
}

#[test]
fn loom_ring_exactly_once_for_competing_consumers() {
    loom::model(|| {
        let ring = Arc::new(LoomRing::with_capacity(4));
        assert!(ring.try_push(100));
        assert!(ring.try_push(200));

        let outputs = Arc::new(Mutex::new(Vec::new()));
        let c1_ring = Arc::clone(&ring);
        let c2_ring = Arc::clone(&ring);
        let c1_out = Arc::clone(&outputs);
        let c2_out = Arc::clone(&outputs);

        let consumer_a = thread::spawn(move || {
            let value = loop {
                if let Some(v) = c1_ring.try_pop() {
                    break v;
                }
                thread::yield_now();
            };
            c1_out.lock().expect("lock").push(value);
        });

        let consumer_b = thread::spawn(move || {
            let value = loop {
                if let Some(v) = c2_ring.try_pop() {
                    break v;
                }
                thread::yield_now();
            };
            c2_out.lock().expect("lock").push(value);
        });

        consumer_a.join().expect("join");
        consumer_b.join().expect("join");

        let mut values = outputs.lock().expect("lock").clone();
        values.sort_unstable();
        assert_eq!(values, vec![100, 200]);
    });
}

#[test]
fn loom_ring_bounded_backpressure_is_observable() {
    loom::model(|| {
        let ring = LoomRing::with_capacity(2);
        assert!(ring.try_push(1));
        assert!(ring.try_push(2));
        assert!(!ring.try_push(3));
        assert_eq!(ring.try_pop(), Some(1));
        assert_eq!(ring.try_pop(), Some(2));
        assert_eq!(ring.try_pop(), None);
    });
}
