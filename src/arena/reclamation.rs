use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetiredRecord {
    pub handle_offset: u64,
    pub retire_epoch: u64,
}

#[derive(Debug, Default)]
pub struct EpochManager {
    global_epoch: AtomicU64,
    active_readers: AtomicU64,
    retired: Mutex<Vec<RetiredRecord>>,
}

pub struct EpochGuard<'a> {
    manager: &'a EpochManager,
}

impl EpochManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            global_epoch: AtomicU64::new(1),
            active_readers: AtomicU64::new(0),
            retired: Mutex::new(Vec::new()),
        }
    }

    pub fn pin(&self) -> EpochGuard<'_> {
        self.active_readers.fetch_add(1, Ordering::AcqRel);
        EpochGuard { manager: self }
    }

    pub fn advance_epoch(&self) -> u64 {
        self.global_epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn retire_offset(&self, handle_offset: u64) {
        let retire_epoch = self.global_epoch.load(Ordering::Acquire);
        let mut retired = self.retired.lock().expect("epoch manager poisoned");
        retired.push(RetiredRecord {
            handle_offset,
            retire_epoch,
        });
    }

    pub fn try_collect(&self) -> Vec<RetiredRecord> {
        // Reclamation is deferred until no readers are pinned in the current epoch window.
        if self.active_readers.load(Ordering::Acquire) != 0 {
            return Vec::new();
        }

        let current = self.global_epoch.load(Ordering::Acquire);
        let mut retired = self.retired.lock().expect("epoch manager poisoned");
        let mut reclaimed = Vec::new();
        let mut i = 0;
        while i < retired.len() {
            if retired[i].retire_epoch < current {
                reclaimed.push(retired.swap_remove(i));
            } else {
                i += 1;
            }
        }
        reclaimed
    }
}

impl Drop for EpochGuard<'_> {
    fn drop(&mut self) {
        self.manager.active_readers.fetch_sub(1, Ordering::AcqRel);
    }
}
