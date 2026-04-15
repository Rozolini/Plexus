#![cfg(windows)]

use std::env;
use std::mem::size_of;
use std::process::Command;

use plexus::{Handle, MapAccess, SharedArena, SharedMapping};

#[repr(C)]
#[derive(Clone, Copy)]
struct TreeNode {
    value: u64,
    left_offset: u64,
    left_generation: u64,
    right_offset: u64,
    right_generation: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RootEnvelope {
    root_offset: u64,
    root_generation: u64,
}

#[test]
#[ignore = "requires subprocess execution permissions"]
fn e2e_two_process_transfer() {
    let marker = env::var("PLEXUS_E2E_CHILD").ok();
    // Child branch writes payload and exits; parent branch validates cross-process visibility.
    if marker.as_deref() == Some("writer") {
        let name = env::var("PLEXUS_E2E_NAME").expect("name");
        let mapping =
            SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("writer mapping");
        // SAFETY: writer owns mapping during process.
        let mut arena = unsafe {
            SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), mapping.created_new())
                .expect("arena")
        };
        arena.begin_publish();
        let _ = arena.allocate_value(2026_u64).expect("write value");
        arena.commit();
        return;
    }

    let name = format!("Local\\plexus-e2e-{}", std::process::id());
    let mapping =
        SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("parent mapping");
    // SAFETY: initialize header once before child attach.
    let _parent_arena_init = unsafe {
        SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), mapping.created_new())
            .expect("parent arena init")
    };
    let exe = env::current_exe().expect("exe");
    let status = Command::new(exe)
        .env("PLEXUS_E2E_CHILD", "writer")
        .env("PLEXUS_E2E_NAME", &name)
        // Re-enter this exact ignored test in child process to execute writer path only.
        .arg("--ignored")
        .arg("--exact")
        .arg("e2e_two_process_transfer")
        .status()
        .expect("spawn");
    assert!(status.success());

    // SAFETY: reader attaches to existing mapping.
    let arena = unsafe {
        SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), false).expect("arena")
    };
    let handle = plexus::Handle::<u64>::from_offset(arena.header().committed_tail - 8);
    let value = arena.read_copy(handle).expect("read");
    assert_eq!(value, 2026);
}

#[test]
#[ignore = "requires subprocess execution permissions"]
fn e2e_tree_transfer_via_handles() {
    let marker = env::var("PLEXUS_E2E_CHILD").ok();
    // Child branch materializes a small tree encoded via offsets+generation handles.
    if marker.as_deref() == Some("tree-writer") {
        let name = env::var("PLEXUS_E2E_NAME").expect("name");
        let mapping =
            SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("writer mapping");
        // SAFETY: writer process owns mapping for this test execution.
        let mut arena = unsafe {
            SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), mapping.created_new())
                .expect("arena")
        };
        arena.begin_publish();
        let left = arena
            .allocate_value(TreeNode {
                value: 7,
                left_offset: 0,
                left_generation: 0,
                right_offset: 0,
                right_generation: 0,
            })
            .expect("left");
        let right = arena
            .allocate_value(TreeNode {
                value: 11,
                left_offset: 0,
                left_generation: 0,
                right_offset: 0,
                right_generation: 0,
            })
            .expect("right");
        let root = arena
            .allocate_value(TreeNode {
                value: 5,
                left_offset: left.offset(),
                left_generation: left.generation(),
                right_offset: right.offset(),
                right_generation: right.generation(),
            })
            .expect("root");
        let _envelope = arena
            .allocate_value(RootEnvelope {
                root_offset: root.offset(),
                root_generation: root.generation(),
            })
            .expect("envelope");
        arena.commit();
        return;
    }

    let name = format!("Local\\plexus-e2e-tree-{}", std::process::id());
    let mapping =
        SharedMapping::create(&name, 1 << 20, MapAccess::ReadWrite).expect("parent mapping");
    // SAFETY: initialize header once before child attach.
    let _parent_arena_init = unsafe {
        SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), mapping.created_new())
            .expect("parent arena init")
    };
    let exe = env::current_exe().expect("exe");
    let status = Command::new(exe)
        .env("PLEXUS_E2E_CHILD", "tree-writer")
        .env("PLEXUS_E2E_NAME", &name)
        .arg("--ignored")
        .arg("--exact")
        .arg("e2e_tree_transfer_via_handles")
        .status()
        .expect("spawn");
    assert!(status.success());

    // SAFETY: reader attaches to shared mapping created by writer.
    let arena = unsafe {
        SharedArena::from_raw_parts(mapping.as_non_null(), mapping.len(), false).expect("arena")
    };
    let envelope_offset = arena.current_tail() - size_of::<RootEnvelope>() as u64;
    let envelope = arena
        .read_copy(Handle::<RootEnvelope>::from_offset(envelope_offset))
        .expect("envelope");
    // Resolve root/children from serialized handle parts rather than absolute pointers.
    let root = arena
        .read_copy(Handle::<TreeNode>::from_parts(
            envelope.root_offset,
            envelope.root_generation,
        ))
        .expect("root");
    let left = arena
        .read_copy(Handle::<TreeNode>::from_parts(
            root.left_offset,
            root.left_generation,
        ))
        .expect("left");
    let right = arena
        .read_copy(Handle::<TreeNode>::from_parts(
            root.right_offset,
            root.right_generation,
        ))
        .expect("right");
    assert_eq!(root.value + left.value + right.value, 23);
}
