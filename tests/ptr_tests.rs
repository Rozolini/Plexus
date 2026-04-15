use std::mem::size_of;
use std::ptr::NonNull;

use plexus::RelPtr;

#[repr(C)]
struct Node {
    next: RelPtr<Node>,
    value: u64,
}

#[test]
fn rel_ptr_roundtrip_in_same_region() {
    let mut nodes = [
        Node {
            next: RelPtr::null(),
            value: 10,
        },
        Node {
            next: RelPtr::null(),
            value: 20,
        },
    ];

    let rel_slot = NonNull::from(&mut nodes[0].next);
    let second = NonNull::from(&mut nodes[1]);
    // SAFETY: both pointers point into the same backing allocation.
    let rel = unsafe { RelPtr::from_ptr(rel_slot, second) };
    nodes[0].next = rel;

    let resolved = nodes[0].next.resolve(rel_slot).expect("must resolve");
    // SAFETY: resolved points to second node in this test.
    let resolved_ref = unsafe { resolved.as_ref() };
    assert_eq!(resolved_ref.value, 20);
}

#[test]
fn rel_ptr_remains_valid_after_simulated_remap_base() {
    let mut nodes = [
        Node {
            next: RelPtr::null(),
            value: 10,
        },
        Node {
            next: RelPtr::null(),
            value: 20,
        },
    ];

    let rel_slot = NonNull::from(&mut nodes[0].next);
    let second = NonNull::from(&mut nodes[1]);
    // SAFETY: both pointers are from the same mapped object graph.
    nodes[0].next = unsafe { RelPtr::from_ptr(rel_slot, second) };

    // Simulate remap by copying bytes to a different base address.
    let src_len = nodes.len() * size_of::<Node>();
    // SAFETY: read-only byte view over initialized nodes.
    let src_bytes = unsafe { std::slice::from_raw_parts(nodes.as_ptr().cast::<u8>(), src_len) };
    let mut remapped = src_bytes.to_vec();
    let remap_nodes = remapped.as_mut_ptr().cast::<Node>();
    // SAFETY: remapped bytes contain exactly two Node values.
    let remap_first = unsafe { &mut *remap_nodes };
    let remap_rel_slot = NonNull::from(&mut remap_first.next);
    let resolved = remap_first
        .next
        .resolve(remap_rel_slot)
        .expect("must resolve after remap");
    // SAFETY: relative pointer points to second copied node.
    let resolved_ref = unsafe { resolved.as_ref() };
    assert_eq!(resolved_ref.value, 20);
}
