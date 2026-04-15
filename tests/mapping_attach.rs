#![cfg(windows)]

use plexus::{MapAccess, MappingError, MappingSecurityProfile, SharedArena, SharedMapping};
use std::thread;
use std::time::Duration;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

#[test]
fn second_attach_observes_existing_mapping_and_data() {
    let name = format!("Local\\plexus-test-{}", std::process::id());
    let first = SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("create first");
    assert!(first.created_new());

    // SAFETY: mapping pointer remains valid while first is alive.
    let mut arena_a = unsafe {
        SharedArena::from_raw_parts(first.as_non_null(), first.len(), first.created_new())
            .expect("arena a")
    };
    arena_a.begin_publish();
    let handle = arena_a.allocate_value(77_u64).expect("allocate");
    arena_a.commit();

    let second =
        SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("attach second");
    assert!(!second.created_new());
    // SAFETY: second mapping points to same named shared segment.
    let arena_b = unsafe {
        SharedArena::from_raw_parts(second.as_non_null(), second.len(), second.created_new())
            .expect("arena b")
    };

    let value = arena_b.read_copy(handle).expect("read from second");
    assert_eq!(value, 77);
}

#[test]
fn rejects_invalid_mapping_name() {
    let result = SharedMapping::create("   ", 4096, MapAccess::ReadWrite);
    assert!(matches!(result, Err(MappingError::InvalidName)));
}

#[test]
fn rejects_namespace_without_local_or_global_prefix() {
    let result = SharedMapping::create("plexus-test-no-prefix", 4096, MapAccess::ReadWrite);
    assert!(matches!(result, Err(MappingError::InvalidNamespace)));
}

#[test]
fn create_with_owner_only_acl_profile_succeeds() {
    let name = format!("Local\\plexus-acl-owner-{}", std::process::id());
    let mapping = SharedMapping::create_with_profile(
        &name,
        4096,
        MapAccess::ReadWrite,
        MappingSecurityProfile::ProducerOwnerOnly,
    );
    assert!(mapping.is_ok());
}

#[test]
fn rejects_zero_sized_mapping() {
    let result = SharedMapping::create("Local\\plexus-zero-size", 0, MapAccess::ReadWrite);
    assert!(matches!(result, Err(MappingError::InvalidSize)));
}

#[test]
fn repeated_attach_detach_cycles_remain_stable() {
    let name = format!("Local\\plexus-cycles-{}", std::process::id());
    for _ in 0..128 {
        let first = SharedMapping::create(&name, 1 << 16, MapAccess::ReadWrite).expect("create");
        let second = SharedMapping::create(&name, 1 << 16, MapAccess::ReadWrite).expect("attach");
        assert_eq!(first.len(), second.len());
        assert!(first.created_new() || !second.created_new());
        drop(second);
        drop(first);
    }
}

#[test]
fn rejects_permission_mismatch_for_named_mapping() {
    let name = format!("Local\\plexus-ro-{}", std::process::id());
    let _read_only =
        SharedMapping::create(&name, 1 << 16, MapAccess::ReadOnly).expect("create ro mapping");
    let rw_attach = SharedMapping::create(&name, 1 << 16, MapAccess::ReadWrite);
    assert!(matches!(rw_attach, Err(MappingError::OsError(_))));
}

#[test]
fn no_handle_leak_across_many_create_map_drop_cycles() {
    let before_a = process_handle_count();
    let before_b = process_handle_count();
    // Account for normal handle jitter from test harness/runtime activity.
    let baseline_jitter = before_a.abs_diff(before_b);
    let before = before_a.max(before_b);
    for i in 0..512_u32 {
        let name = format!("Local\\plexus-leak-{}-{}", std::process::id(), i);
        let mapping =
            SharedMapping::create(&name, 1 << 16, MapAccess::ReadWrite).expect("create mapping");
        let second =
            SharedMapping::create(&name, 1 << 16, MapAccess::ReadWrite).expect("attach mapping");
        drop(second);
        drop(mapping);
    }
    let allowed_growth = baseline_jitter.saturating_add(8);
    let upper_bound = before.saturating_add(allowed_growth);

    // Give the runtime a short window to finalize transient resources.
    let mut after = process_handle_count();
    for _ in 0..5 {
        if after <= upper_bound {
            break;
        }
        thread::sleep(Duration::from_millis(20));
        after = process_handle_count();
    }
    assert!(
        after <= upper_bound,
        "possible handle leak detected: before={before}, after={after}, allowed_growth={allowed_growth}"
    );
}

fn process_handle_count() -> u32 {
    let mut count = 0_u32;
    // SAFETY: querying current process handle count with valid output pointer.
    let ok = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
    assert_ne!(ok, 0, "GetProcessHandleCount failed");
    count
}
