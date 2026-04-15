use std::sync::atomic::{Ordering, fence};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use thiserror::Error;

use crate::arena::{ArenaError, SharedArena};
use crate::ptr::Handle;
use crate::sync::ring_mpmc::{RingError, SharedRing};
use crate::telemetry::Metrics;
#[cfg(windows)]
use crate::{MapAccess, MappingError, MappingSecurityProfile, SharedMapping};

pub struct SharedChannel<T> {
    ring: Arc<SharedRing<Handle<T>>>,
}

pub struct Producer<T> {
    ring: Arc<SharedRing<Handle<T>>>,
}

pub struct Consumer<T> {
    ring: Arc<SharedRing<Handle<T>>>,
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("arena operation failed: {0}")]
    Arena(#[from] ArenaError),
    #[error("ring operation failed: {0}")]
    Ring(#[from] RingError),
    #[error("synchronization poisoned")]
    PoisonedLock,
    #[cfg(windows)]
    #[error("mapping operation failed: {0}")]
    Mapping(#[from] MappingError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Recoverable,
    Fatal,
    Programmer,
}

pub struct SharedEndpoint<T> {
    arena: Arc<Mutex<SharedArena>>,
    producer: Producer<T>,
    consumer: Consumer<T>,
    metrics: Arc<Metrics>,
    #[cfg(windows)]
    mapping: Option<SharedMapping>,
}

impl<T> SharedChannel<T> {
    pub fn new(capacity: usize) -> Result<Self, RingError> {
        let ring = Arc::new(SharedRing::with_capacity(capacity)?);
        Ok(Self { ring })
    }

    #[must_use]
    pub fn split(&self) -> (Producer<T>, Consumer<T>) {
        (
            Producer {
                ring: Arc::clone(&self.ring),
            },
            Consumer {
                ring: Arc::clone(&self.ring),
            },
        )
    }
}

impl<T> Producer<T> {
    pub fn send(&self, handle: Handle<T>) -> Result<(), RingError> {
        self.ring.try_push(handle)
    }
}

impl<T> Consumer<T> {
    pub fn try_recv(&self) -> Result<Handle<T>, RingError> {
        self.ring.try_pop()
    }
}

impl<T> SharedEndpoint<T>
where
    T: Copy,
{
    pub fn new(arena: SharedArena, channel_capacity: usize) -> Result<Self, IpcError> {
        let channel = SharedChannel::<T>::new(channel_capacity)?;
        let (producer, consumer) = channel.split();
        Ok(Self {
            arena: Arc::new(Mutex::new(arena)),
            producer,
            consumer,
            metrics: Arc::new(Metrics::default()),
            #[cfg(windows)]
            mapping: None,
        })
    }

    #[cfg(windows)]
    pub fn from_shared_mapping(
        mapping: SharedMapping,
        channel_capacity: usize,
    ) -> Result<Self, IpcError> {
        let arena = unsafe {
            // SAFETY[INV-MAP-LIFETIME]: endpoint keeps mapping alive in `self.mapping`.
            SharedArena::from_raw_parts(
                mapping.as_non_null(),
                mapping.len(),
                mapping.created_new(),
            )?
        };
        let mut endpoint = Self::new(arena, channel_capacity)?;
        endpoint.mapping = Some(mapping);
        Ok(endpoint)
    }

    #[cfg(windows)]
    pub fn open_or_create_mapping(
        name: &str,
        mapping_len: usize,
        access: MapAccess,
        channel_capacity: usize,
    ) -> Result<Self, IpcError> {
        let mapping = SharedMapping::create(name, mapping_len, access)?;
        Self::from_shared_mapping(mapping, channel_capacity)
    }

    #[cfg(windows)]
    pub fn open_or_create_mapping_with_profile(
        name: &str,
        mapping_len: usize,
        access: MapAccess,
        profile: MappingSecurityProfile,
        channel_capacity: usize,
    ) -> Result<Self, IpcError> {
        let mapping = SharedMapping::create_with_profile(name, mapping_len, access, profile)?;
        Self::from_shared_mapping(mapping, channel_capacity)
    }

    #[must_use]
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }

    pub fn send_copy(&self, value: T) -> Result<(), IpcError> {
        let started = Instant::now();
        let mut arena = self.arena.lock().map_err(|_| IpcError::PoisonedLock)?;
        arena.begin_publish();
        let handle = arena.allocate_value(value)?;
        // Keep protocol explicit: write -> fence -> commit bit -> notify.
        fence(Ordering::Release);
        arena.commit();
        if let Err(error) = self.producer.send(handle) {
            self.metrics.on_enqueue_full();
            return Err(IpcError::Ring(error));
        }

        self.metrics
            .observe_send_latency_ns(started.elapsed().as_nanos() as u64);
        self.metrics.on_enqueue_ok();
        self.metrics
            .observe_queue_depth(self.producer.ring.len_estimate() as u64);
        Ok(())
    }

    pub fn try_recv_copy(&self) -> Result<T, IpcError> {
        let handle = self.consumer.try_recv().map_err(|error| {
            self.metrics.on_dequeue_empty();
            IpcError::Ring(error)
        })?;
        // Reader side of protocol: observe commit/notify -> fence -> read payload.
        fence(Ordering::Acquire);
        let arena = self.arena.lock().map_err(|_| IpcError::PoisonedLock)?;
        let value = arena.read_copy(handle)?;
        self.metrics.on_dequeue_ok();
        self.metrics
            .observe_queue_depth(self.consumer.ring.len_estimate() as u64);
        Ok(value)
    }
}

impl IpcError {
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::Ring(RingError::Full | RingError::Empty) => ErrorClass::Recoverable,
            Self::Arena(
                ArenaError::BadLayout | ArenaError::OutOfBounds(_) | ArenaError::StaleHandle,
            ) => ErrorClass::Programmer,
            #[cfg(windows)]
            Self::Mapping(_) => ErrorClass::Fatal,
            Self::PoisonedLock | Self::Arena(_) => ErrorClass::Fatal,
            Self::Ring(_) => ErrorClass::Recoverable,
        }
    }
}
