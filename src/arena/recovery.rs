use crate::arena::header::{ArenaHeader, PublishState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryReport {
    Healthy,
    HeaderCorrupted,
    TailOutOfBounds,
    PendingPublication,
}

impl RecoveryReport {
    #[must_use]
    pub fn inspect(header: &ArenaHeader) -> Self {
        if !header.is_valid() {
            return Self::HeaderCorrupted;
        }

        if header.committed_tail > header.arena_size {
            return Self::TailOutOfBounds;
        }

        if header.prepared_tail > header.arena_size {
            return Self::TailOutOfBounds;
        }

        if header.state() == PublishState::Preparing {
            return Self::PendingPublication;
        }

        Self::Healthy
    }

    pub fn recover_in_place(header: &mut ArenaHeader) -> Self {
        Self::recover_in_place_with_timeout(header, u64::MAX, 0)
    }

    pub fn recover_in_place_with_timeout(
        header: &mut ArenaHeader,
        now_ns: u64,
        timeout_ns: u64,
    ) -> Self {
        if !header.is_valid() {
            return Self::HeaderCorrupted;
        }

        if header.committed_tail > header.arena_size || header.prepared_tail > header.arena_size {
            return Self::TailOutOfBounds;
        }

        if header.state() == PublishState::Preparing && header.is_owner_stale(now_ns, timeout_ns) {
            // Writer likely died mid-publish: roll back to last committed tail.
            header.rollback_to_committed();
            return Self::PendingPublication;
        }

        Self::Healthy
    }
}
