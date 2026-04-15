use std::alloc::Layout;
use std::ptr::NonNull;

use plexus::{ArenaError, SharedArena};

#[test]
fn stale_handle_is_rejected_after_deallocate() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test owns this memory.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };

    let handle = arena.allocate_value(123_u64).expect("alloc");
    arena
        .deallocate_layout(handle, Layout::new::<u64>())
        .expect("deallocate");
    let read = arena.read_copy(handle);
    assert!(matches!(read, Err(ArenaError::StaleHandle)));
}
