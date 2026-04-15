use core::alloc::Layout;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
#[cfg(not(miri))]
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::arena::header::{ARENA_HEADER_SIZE, ArenaHeader};
use crate::arena::recovery::RecoveryReport;
use crate::ptr::Handle;

#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("arena is too small")]
    TooSmall,
    #[error("out of shared memory")]
    OutOfMemory,
    #[error("offset {0} is out of bounds")]
    OutOfBounds(usize),
    #[error("layout error")]
    BadLayout,
    #[error("arena header is corrupted")]
    CorruptedHeader,
    #[error("stale handle generation detected")]
    StaleHandle,
}

#[derive(Debug)]
pub struct SharedArena {
    base: NonNull<u8>,
    len: usize,
    tail: AtomicU64,
    free_list: Mutex<Vec<FreeBlock>>,
    generations: Mutex<HashMap<u64, u64>>,
}

#[derive(Debug, Clone, Copy)]
struct FreeBlock {
    offset: u64,
    size: usize,
}

impl SharedArena {
    /// # Safety
    /// Caller must guarantee `base..base+len` points to a writable mapped region
    /// that remains valid for the entire lifetime of this arena.
    /// Invariants: `INV-MAP-LIFETIME`, `INV-ARENA-HEADER`.
    pub unsafe fn from_raw_parts(
        base: NonNull<u8>,
        len: usize,
        initialize: bool,
    ) -> Result<Self, ArenaError> {
        if len < ARENA_HEADER_SIZE + align_of::<u64>() {
            return Err(ArenaError::TooSmall);
        }

        let mut arena = Self {
            base,
            len,
            tail: AtomicU64::new(ARENA_HEADER_SIZE as u64),
            free_list: Mutex::new(Vec::new()),
            generations: Mutex::new(HashMap::new()),
        };

        if initialize {
            arena.initialize_header();
        } else {
            let report = RecoveryReport::recover_in_place_with_timeout(
                arena.header_mut(),
                now_monotonic_ns(),
                Duration::from_secs(5).as_nanos() as u64,
            );
            if matches!(
                report,
                RecoveryReport::HeaderCorrupted | RecoveryReport::TailOutOfBounds
            ) {
                return Err(ArenaError::CorruptedHeader);
            }
            let committed_tail = arena.header().committed_tail;
            arena.tail.store(committed_tail, Ordering::Release);
        }

        Ok(arena)
    }

    fn initialize_header(&self) {
        let header = ArenaHeader::new(self.len, std::process::id(), now_monotonic_ns());
        // SAFETY[INV-ARENA-HEADER]: header space is inside mapping and aligned.
        unsafe { self.base.as_ptr().cast::<ArenaHeader>().write(header) };
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn header(&self) -> &ArenaHeader {
        // SAFETY[INV-ARENA-HEADER]: header is placed at offset 0 for mapping lifetime.
        unsafe { &*self.base.as_ptr().cast::<ArenaHeader>() }
    }

    fn header_mut(&mut self) -> &mut ArenaHeader {
        // SAFETY[INV-ARENA-HEADER]: exclusive access through &mut self.
        unsafe { &mut *self.base.as_ptr().cast::<ArenaHeader>() }
    }

    pub fn allocate_layout(&self, layout: Layout) -> Result<Handle<u8>, ArenaError> {
        if !layout.align().is_power_of_two() {
            return Err(ArenaError::BadLayout);
        }

        if let Some(handle) = self.try_allocate_from_free_list(layout)? {
            return Ok(handle);
        }

        loop {
            let current = self.tail.load(Ordering::Acquire) as usize;
            let aligned = align_up(current, layout.align());
            let next = aligned
                .checked_add(layout.size())
                .ok_or(ArenaError::OutOfMemory)?;
            if next > self.len {
                return Err(ArenaError::OutOfMemory);
            }

            if self
                .tail
                .compare_exchange(
                    current as u64,
                    next as u64,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let generation = self.current_generation_for_offset(aligned as u64);
                return Ok(Handle::from_parts(aligned as u64, generation));
            }
        }
    }

    pub fn allocate_value<T>(&self, value: T) -> Result<Handle<T>, ArenaError> {
        let layout = Layout::new::<T>();
        let handle = self.allocate_layout(layout)?.cast::<T>();
        // SAFETY[INV-ARENA-BOUNDS]: allocated slot has size/layout for T and lies in mapping.
        unsafe { self.write(&handle, value)? };
        Ok(handle)
    }

    /// # Safety
    /// `handle` must have been returned by this arena for type `T`.
    /// Invariant: `INV-ARENA-BOUNDS`.
    pub unsafe fn write<T>(&self, handle: &Handle<T>, value: T) -> Result<(), ArenaError> {
        let ptr = self.ptr_from_handle(handle)?;
        // SAFETY[INV-ARENA-BOUNDS]: caller contract + bounds check in ptr_from_handle.
        unsafe { ptr.as_ptr().write(value) };
        Ok(())
    }

    pub fn read_copy<T: Copy>(&self, handle: Handle<T>) -> Result<T, ArenaError> {
        let ptr = self.ptr_from_handle(&handle)?;
        // SAFETY[INV-ARENA-BOUNDS]: bounds checked pointer, Copy avoids drop ownership issues.
        Ok(unsafe { ptr.as_ptr().read() })
    }

    pub fn commit(&mut self) {
        let committed = self.tail.load(Ordering::Acquire);
        let header = self.header_mut();
        header.mark_commit(committed, std::process::id(), now_monotonic_ns());
    }

    pub fn begin_publish(&mut self) {
        let prepared = self.tail.load(Ordering::Acquire);
        let header = self.header_mut();
        header.mark_prepare(prepared, std::process::id(), now_monotonic_ns());
    }

    pub fn abort_publish(&mut self) {
        let committed = self.header().committed_tail;
        self.tail.store(committed, Ordering::Release);
        let header = self.header_mut();
        header.rollback_to_committed();
    }

    #[must_use]
    pub fn current_tail(&self) -> u64 {
        self.tail.load(Ordering::Acquire)
    }

    pub fn heartbeat(&mut self) {
        let header = self.header_mut();
        header.heartbeat(std::process::id(), now_monotonic_ns());
    }

    #[must_use]
    pub fn detect_dead_owner(&self, timeout: Duration) -> bool {
        let header = self.header();
        header.is_owner_stale(now_monotonic_ns(), timeout.as_nanos() as u64)
    }

    pub fn cleanup_orphaned_publication(&mut self, timeout: Duration) -> RecoveryReport {
        let report = RecoveryReport::recover_in_place_with_timeout(
            self.header_mut(),
            now_monotonic_ns(),
            timeout.as_nanos() as u64,
        );
        if matches!(report, RecoveryReport::PendingPublication) {
            let committed = self.header().committed_tail;
            self.tail.store(committed, Ordering::Release);
        }
        report
    }

    pub fn deallocate_layout<T>(
        &self,
        handle: Handle<T>,
        layout: Layout,
    ) -> Result<(), ArenaError> {
        self.validate_generation(&handle)?;
        let mut free_list = self
            .free_list
            .lock()
            .map_err(|_| ArenaError::CorruptedHeader)?;
        free_list.push(FreeBlock {
            offset: handle.offset(),
            size: layout.size(),
        });
        drop(free_list);
        self.bump_generation(handle.offset());
        Ok(())
    }

    pub fn ptr_from_handle<T>(&self, handle: &Handle<T>) -> Result<NonNull<T>, ArenaError> {
        self.validate_generation(handle)?;
        let offset = handle.offset() as usize;
        if offset + size_of::<T>() > self.len {
            return Err(ArenaError::OutOfBounds(offset));
        }

        // SAFETY[INV-ARENA-BOUNDS]: offset bounds checked against mapping length.
        let ptr = unsafe { self.base.as_ptr().add(offset).cast::<T>() };
        NonNull::new(ptr).ok_or(ArenaError::OutOfBounds(offset))
    }

    fn try_allocate_from_free_list(
        &self,
        layout: Layout,
    ) -> Result<Option<Handle<u8>>, ArenaError> {
        let mut free_list = self
            .free_list
            .lock()
            .map_err(|_| ArenaError::CorruptedHeader)?;
        if let Some((idx, block)) = free_list
            .iter()
            .enumerate()
            .find(|(_, block)| {
                block.size >= layout.size()
                    && (block.offset as usize).is_multiple_of(layout.align())
            })
            .map(|(idx, block)| (idx, *block))
        {
            free_list.swap_remove(idx);
            let generation = self.current_generation_for_offset(block.offset);
            return Ok(Some(Handle::from_parts(block.offset, generation)));
        }
        Ok(None)
    }

    fn current_generation_for_offset(&self, offset: u64) -> u64 {
        let mut generations = self
            .generations
            .lock()
            .expect("arena generation map poisoned");
        let generation = generations.entry(offset).or_insert(1);
        *generation
    }

    fn bump_generation(&self, offset: u64) {
        let mut generations = self
            .generations
            .lock()
            .expect("arena generation map poisoned");
        let generation = generations.entry(offset).or_insert(1);
        *generation = generation.saturating_add(1);
    }

    fn validate_generation<T>(&self, handle: &Handle<T>) -> Result<(), ArenaError> {
        let generations = self
            .generations
            .lock()
            .map_err(|_| ArenaError::CorruptedHeader)?;
        let current = generations.get(&handle.offset()).copied().unwrap_or(1);
        if current != handle.generation() {
            return Err(ArenaError::StaleHandle);
        }
        Ok(())
    }
}

fn align_up(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + (alignment - 1)) & !(alignment - 1)
}

#[cfg(not(miri))]
fn now_monotonic_ns() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_nanos() as u64
}

#[cfg(miri)]
fn now_monotonic_ns() -> u64 {
    // Miri isolation forbids realtime clock syscalls; a monotonic counter is sufficient
    // for timeout and staleness ordering semantics used in tests.
    static MIRI_MONOTONIC_NS: AtomicU64 = AtomicU64::new(1);
    MIRI_MONOTONIC_NS.fetch_add(1, Ordering::Relaxed)
}
