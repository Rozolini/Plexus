use core::mem::size_of;

pub const ARENA_MAGIC: u32 = 0x504C_4558;
pub const ARENA_HEADER_SIZE: usize = size_of::<ArenaHeader>();

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaVersion {
    pub major: u16,
    pub minor: u16,
}

impl ArenaVersion {
    pub const CURRENT: Self = Self { major: 0, minor: 1 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityResult {
    Compatible,
    ForwardCompatibleReadOnly,
    IncompatibleMajor,
    IncompatibleFeatureFlags,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishState {
    Clean = 0,
    Preparing = 1,
}

impl PublishState {
    #[must_use]
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::Clean),
            1 => Some(Self::Preparing),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArenaHeader {
    pub magic: u32,
    pub version: ArenaVersion,
    pub arena_size: u64,
    pub feature_flags: u64,
    pub epoch: u64,
    pub owner_pid: u32,
    pub _reserved_owner: u32,
    pub last_heartbeat_ns: u64,
    pub prepared_tail: u64,
    pub committed_tail: u64,
    pub publish_state: u32,
    pub _reserved0: u32,
    pub checksum: u32,
    pub _reserved1: u32,
}

impl ArenaHeader {
    pub fn new(arena_size: usize, owner_pid: u32, now_ns: u64) -> Self {
        let mut header = Self {
            magic: ARENA_MAGIC,
            version: ArenaVersion::CURRENT,
            arena_size: arena_size as u64,
            feature_flags: 0,
            epoch: 1,
            owner_pid,
            _reserved_owner: 0,
            last_heartbeat_ns: now_ns,
            prepared_tail: ARENA_HEADER_SIZE as u64,
            committed_tail: ARENA_HEADER_SIZE as u64,
            publish_state: PublishState::Clean as u32,
            _reserved0: 0,
            checksum: 0,
            _reserved1: 0,
        };
        header.checksum = header.calculate_checksum();
        header
    }

    #[must_use]
    pub fn calculate_checksum(&self) -> u32 {
        // Lightweight integrity check over header control fields only.
        // Payload bytes are intentionally excluded to keep attach-time validation cheap.
        let mut sum = self.magic
            ^ self.arena_size as u32
            ^ (self.arena_size >> 32) as u32
            ^ self.feature_flags as u32
            ^ (self.feature_flags >> 32) as u32
            ^ self.epoch as u32
            ^ (self.epoch >> 32) as u32
            ^ self.owner_pid
            ^ self.last_heartbeat_ns as u32
            ^ (self.last_heartbeat_ns >> 32) as u32
            ^ self.prepared_tail as u32
            ^ (self.prepared_tail >> 32) as u32
            ^ self.committed_tail as u32
            ^ (self.committed_tail >> 32) as u32
            ^ self.publish_state;
        sum ^= u32::from(self.version.major) << 16 | u32::from(self.version.minor);
        sum
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.magic == ARENA_MAGIC
            && PublishState::from_raw(self.publish_state).is_some()
            && self.checksum == self.calculate_checksum()
    }

    #[must_use]
    pub fn state(&self) -> PublishState {
        PublishState::from_raw(self.publish_state).unwrap_or(PublishState::Preparing)
    }

    pub fn mark_prepare(&mut self, prepared_tail: u64, owner_pid: u32, now_ns: u64) {
        self.owner_pid = owner_pid;
        self.last_heartbeat_ns = now_ns;
        self.prepared_tail = prepared_tail;
        self.publish_state = PublishState::Preparing as u32;
        self.checksum = self.calculate_checksum();
    }

    pub fn mark_commit(&mut self, committed_tail: u64, owner_pid: u32, now_ns: u64) {
        self.owner_pid = owner_pid;
        self.last_heartbeat_ns = now_ns;
        self.committed_tail = committed_tail;
        self.prepared_tail = committed_tail;
        self.publish_state = PublishState::Clean as u32;
        self.epoch = self.epoch.saturating_add(1);
        self.checksum = self.calculate_checksum();
    }

    pub fn rollback_to_committed(&mut self) {
        self.prepared_tail = self.committed_tail;
        self.publish_state = PublishState::Clean as u32;
        self.checksum = self.calculate_checksum();
    }

    pub fn heartbeat(&mut self, owner_pid: u32, now_ns: u64) {
        self.owner_pid = owner_pid;
        self.last_heartbeat_ns = now_ns;
        self.checksum = self.calculate_checksum();
    }

    #[must_use]
    pub fn is_owner_stale(&self, now_ns: u64, timeout_ns: u64) -> bool {
        now_ns.saturating_sub(self.last_heartbeat_ns) > timeout_ns
    }

    #[must_use]
    pub fn compatibility_with(
        &self,
        expected: ArenaVersion,
        supported_flags_mask: u64,
    ) -> CompatibilityResult {
        // Major is a hard break; reject to avoid interpreting incompatible layouts.
        if self.version.major != expected.major {
            return CompatibilityResult::IncompatibleMajor;
        }

        // Unknown required flags are treated as incompatible capabilities.
        if self.feature_flags & !supported_flags_mask != 0 {
            return CompatibilityResult::IncompatibleFeatureFlags;
        }

        // Newer minor is accepted as read-only if capability mask already matched.
        if self.version.minor > expected.minor {
            return CompatibilityResult::ForwardCompatibleReadOnly;
        }

        CompatibilityResult::Compatible
    }
}
