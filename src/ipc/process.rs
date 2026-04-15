#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRole {
    Producer,
    Consumer,
    Mixed,
}

#[derive(Debug, Clone)]
pub struct ProcessIdentity {
    pub process_id: u32,
    pub role: PeerRole,
    pub segment_name: String,
}

impl ProcessIdentity {
    #[must_use]
    pub fn current(role: PeerRole, segment_name: impl Into<String>) -> Self {
        Self {
            process_id: std::process::id(),
            role,
            segment_name: segment_name.into(),
        }
    }
}
