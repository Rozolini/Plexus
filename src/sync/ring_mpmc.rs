use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RingError {
    #[error("ring capacity must be a power of two and > 1")]
    InvalidCapacity,
    #[error("ring is currently full")]
    Full,
    #[error("ring is currently empty")]
    Empty,
}

#[repr(C)]
struct Slot<T> {
    sequence: AtomicU64,
    value: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY[INV-RING-SLOT]: interior mutability is synchronized by slot sequence protocol.
unsafe impl<T: Send> Send for Slot<T> {}
// SAFETY[INV-RING-SLOT]: readers/writers coordinate through atomics.
unsafe impl<T: Send> Sync for Slot<T> {}

pub struct SharedRing<T> {
    mask: u64,
    capacity: u64,
    head: AtomicU64,
    tail: AtomicU64,
    slots: Box<[Slot<T>]>,
}

impl<T> SharedRing<T> {
    pub fn with_capacity(capacity: usize) -> Result<Self, RingError> {
        if capacity <= 1 || !capacity.is_power_of_two() {
            return Err(RingError::InvalidCapacity);
        }

        let mut slots = Vec::with_capacity(capacity);
        for i in 0..capacity as u64 {
            slots.push(Slot {
                sequence: AtomicU64::new(i),
                value: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }

        Ok(Self {
            mask: (capacity - 1) as u64,
            capacity: capacity as u64,
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            slots: slots.into_boxed_slice(),
        })
    }

    pub fn try_push(&self, value: T) -> Result<(), RingError> {
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let slot = &self.slots[(tail & self.mask) as usize];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let diff = sequence as i64 - tail as i64;

            if diff == 0 {
                if self
                    .tail
                    .compare_exchange_weak(tail, tail + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // SAFETY[INV-RING-SLOT]: producer owns slot when seq == tail and CAS succeeds.
                    unsafe {
                        (*slot.value.get()).write(value);
                    }
                    slot.sequence.store(tail + 1, Ordering::Release);
                    return Ok(());
                }
            } else if diff < 0 {
                return Err(RingError::Full);
            } else {
                std::hint::spin_loop();
            }
        }
    }

    pub fn try_pop(&self) -> Result<T, RingError> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let slot = &self.slots[(head & self.mask) as usize];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let diff = sequence as i64 - (head + 1) as i64;

            if diff == 0 {
                if self
                    .head
                    .compare_exchange_weak(head, head + 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // SAFETY[INV-RING-SLOT]: consumer owns slot when seq == head + 1 and CAS succeeds.
                    let value = unsafe { (*slot.value.get()).assume_init_read() };
                    slot.sequence.store(head + self.capacity, Ordering::Release);
                    return Ok(value);
                }
            } else if diff < 0 {
                return Err(RingError::Empty);
            } else {
                std::hint::spin_loop();
            }
        }
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity as usize
    }

    #[must_use]
    pub fn len_estimate(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        tail.saturating_sub(head) as usize
    }
}

impl<T> Drop for SharedRing<T> {
    fn drop(&mut self) {
        while let Ok(value) = self.try_pop() {
            drop(value);
        }
    }
}
