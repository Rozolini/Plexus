#![allow(clippy::arc_with_non_send_sync)]

use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use plexus::{PlexusMap, PlexusString, PlexusVec, SharedArena};

#[test]
fn plexus_vec_roundtrip() {
    let mut backing = vec![0_u8; 8192];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: owned by test process.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let arena = Arc::new(Mutex::new(arena));

    let mut vec = PlexusVec::new(Arc::clone(&arena));
    vec.push(1_u64).expect("push");
    vec.push(2_u64).expect("push");
    assert_eq!(vec.get(1).expect("get"), Some(2));
}

#[test]
fn plexus_string_roundtrip() {
    let mut backing = vec![0_u8; 8192];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: owned by test process.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let arena = Arc::new(Mutex::new(arena));

    let mut value = PlexusString::new(Arc::clone(&arena));
    value.push_str("plexus").expect("push");
    assert_eq!(value.as_string().expect("string"), "plexus");
}

#[test]
fn plexus_map_roundtrip() {
    let mut backing = vec![0_u8; 8192];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: owned by test process.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let arena = Arc::new(Mutex::new(arena));

    let mut map = PlexusMap::new(Arc::clone(&arena));
    map.insert(7_u32, 99_u64).expect("insert");
    assert_eq!(map.get(7_u32).expect("get"), Some(99_u64));
}
