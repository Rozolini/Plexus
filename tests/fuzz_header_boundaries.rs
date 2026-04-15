use rand::Rng;

use plexus::{ArenaHeader, RecoveryReport};

#[test]
fn fuzz_header_boundary_validation() {
    let mut rng = rand::rng();
    for _ in 0..10_000 {
        let mut header = ArenaHeader::new(1 << 20, 1234, rng.random());
        header.magic = rng.random();
        header.arena_size = rng.random();
        header.prepared_tail = rng.random();
        header.committed_tail = rng.random();
        header.publish_state = rng.random_range(0..=3);
        header.checksum = rng.random();

        let _ = RecoveryReport::inspect(&header);
    }
}
