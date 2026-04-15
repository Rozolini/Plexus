use std::ptr::NonNull;

use plexus::{RecoveryReport, SharedArena};

fn main() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: diagnostic tool owns this raw region.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let report = RecoveryReport::inspect(arena.header());
    println!("recovery_report={report:?}");
}
