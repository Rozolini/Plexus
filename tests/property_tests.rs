use std::alloc::Layout;
use std::mem::size_of;
use std::ptr::NonNull;

use rand::Rng;

use plexus::SharedArena;

#[test]
fn property_alignment_and_bounds_hold_for_random_layouts() {
    // Use u64 backing so base pointer alignment satisfies ArenaHeader requirements under Miri.
    let mut backing = vec![0_u64; (1 << 20) / size_of::<u64>()];
    let base = NonNull::new(backing.as_mut_ptr().cast::<u8>()).expect("non-null");
    let bytes_len = backing.len() * size_of::<u64>();
    // SAFETY: test owns memory backing.
    let arena = unsafe { SharedArena::from_raw_parts(base, bytes_len, true).expect("arena") };
    let mut rng = rand::rng();

    for _ in 0..2048 {
        let size = rng.random_range(1..128);
        let align_pow = rng.random_range(0..=6);
        let align = 1usize << align_pow;
        let layout = Layout::from_size_align(size, align).expect("layout");
        let handle = arena.allocate_layout(layout).expect("allocate");
        assert_eq!(handle.offset() as usize % align, 0);
        assert!(handle.offset() as usize + size <= arena.len());
    }
}
