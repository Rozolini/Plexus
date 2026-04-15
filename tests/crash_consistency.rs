use std::ptr::NonNull;
use std::time::Duration;

use plexus::{PublishState, RecoveryReport, SharedArena};

#[test]
fn pending_publication_is_rolled_back_on_reattach() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");

    let (committed_tail, orphan_handle_offset) = {
        // SAFETY: test owns and controls the backing slice.
        let mut arena =
            unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };
        let first = arena.allocate_value(7_u64).expect("allocate first");
        arena.begin_publish();
        arena.commit();
        assert_eq!(arena.read_copy(first).expect("read first"), 7);

        let committed_tail = arena.current_tail();
        arena.begin_publish();
        // Simulate crash after prepare/allocation and before commit.
        let orphan = arena.allocate_value(99_u64).expect("allocate orphan");
        assert_eq!(arena.header().state(), PublishState::Preparing);
        (committed_tail, orphan.offset())
    };

    // SAFETY: same backing memory remapped by another "process" in test.
    let mut recovered =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), false).expect("reopen") };
    let report = recovered.cleanup_orphaned_publication(Duration::from_nanos(1));
    assert_eq!(report, RecoveryReport::PendingPublication);
    assert_eq!(recovered.header().state(), PublishState::Clean);
    assert_eq!(recovered.current_tail(), committed_tail);

    let next = recovered
        .allocate_value(123_u64)
        .expect("allocate after recover");
    assert_eq!(next.offset(), orphan_handle_offset);
}

#[test]
fn committed_publication_survives_writer_crash_before_notify() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");

    let committed_tail = {
        // SAFETY: test owns and controls mapping bytes.
        let mut arena =
            unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };
        arena.begin_publish();
        let _ = arena
            .allocate_value(55_u64)
            .expect("allocate committed value");
        arena.commit();
        arena.current_tail()
    };

    // SAFETY: same memory reopened as reattach path after simulated crash.
    let recovered =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), false).expect("reopen") };
    assert_eq!(recovered.header().state(), PublishState::Clean);
    assert_eq!(recovered.current_tail(), committed_tail);

    let handle =
        plexus::Handle::<u64>::from_offset(committed_tail - std::mem::size_of::<u64>() as u64);
    assert_eq!(recovered.read_copy(handle).expect("read committed"), 55);
}

#[test]
fn stale_owner_orphan_cleanup_rolls_back_prepared_tail() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test owns and controls mapping bytes.
    let mut arena =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };

    arena.begin_publish();
    let orphan = arena.allocate_value(9_u64).expect("orphan");
    assert_eq!(arena.header().state(), PublishState::Preparing);

    std::thread::sleep(Duration::from_millis(2));
    let report = arena.cleanup_orphaned_publication(Duration::from_nanos(1));
    assert_eq!(report, RecoveryReport::PendingPublication);
    assert_eq!(arena.header().state(), PublishState::Clean);

    let next = arena.allocate_value(17_u64).expect("post cleanup alloc");
    assert_eq!(next.offset(), orphan.offset());
}

#[test]
fn recovery_on_reattach_is_deterministic_across_cycles() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");

    let committed_tail = {
        // SAFETY: test owns and controls mapping bytes.
        let mut arena =
            unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena init") };
        arena.begin_publish();
        let _ = arena.allocate_value(77_u64).expect("allocate committed");
        arena.commit();
        let committed = arena.current_tail();
        arena.begin_publish();
        let _ = arena.allocate_value(88_u64).expect("allocate orphan");
        committed
    };

    for idx in 0..5 {
        // SAFETY: deterministic reopen against same bytes.
        let mut recovered =
            unsafe { SharedArena::from_raw_parts(base, backing.len(), false).expect("reopen") };
        let report = recovered.cleanup_orphaned_publication(Duration::from_nanos(1));
        // First attach performs rollback; later attaches should remain healthy and idempotent.
        if idx == 0 {
            assert_eq!(report, RecoveryReport::PendingPublication);
        } else {
            assert_eq!(report, RecoveryReport::Healthy);
        }
        assert_eq!(recovered.header().state(), PublishState::Clean);
        assert_eq!(recovered.current_tail(), committed_tail);
    }
}
