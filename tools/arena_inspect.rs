use std::ptr::NonNull;

use plexus::{ArenaHeader, RecoveryReport, SharedArena};

fn main() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: this helper owns the raw backing bytes.
    let mut arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    arena.commit();

    let header: &ArenaHeader = arena.header();
    let report = RecoveryReport::inspect(header);
    println!("header={header:?}");
    println!("recovery={report:?}");
}
