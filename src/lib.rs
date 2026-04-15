#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all, clippy::pedantic)]
#![allow(
    clippy::arc_with_non_send_sync,
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::comparison_chain,
    clippy::len_without_is_empty,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::needless_late_init,
    clippy::needless_pass_by_value,
    clippy::ptr_cast_constness,
    clippy::pub_underscore_fields,
    clippy::redundant_else,
    clippy::ref_as_ptr,
    clippy::unnecessary_cast
)]

pub mod arena;
pub mod containers;
pub mod ipc;
pub mod ptr;
pub mod sync;
pub mod sys;
pub mod telemetry;

pub use arena::{
    ArenaError, ArenaHeader, ArenaVersion, CompatibilityResult, EpochGuard, EpochManager,
    PublishState, RecoveryReport, RetiredRecord, SharedArena,
};
pub use containers::{PlexusMap, PlexusString, PlexusVec};
pub use ipc::{Consumer, ErrorClass, IpcError, Producer, SharedChannel, SharedEndpoint};
pub use ptr::{Handle, RelPtr};
pub use sync::ring_mpmc::{RingError, SharedRing};
#[cfg(windows)]
pub use sys::{MapAccess, MappingError, MappingMetadata, MappingSecurityProfile, SharedMapping};

pub type ArenaHandle<T> = Handle<T>;
