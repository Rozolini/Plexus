use std::env;
use std::ptr::NonNull;
use std::time::Instant;

use plexus::{SharedArena, SharedChannel};

fn main() {
    let mut backing = vec![0_u8; 1 << 20];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: benchmark owns backing storage for arena lifetime.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let channel = SharedChannel::<u64>::new(1024).expect("channel");
    let (producer, consumer) = channel.split();

    let start = Instant::now();
    let messages = env::var("PLEXUS_BENCH_ITERATIONS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000_u64);
    for i in 0..messages {
        // Benchmark measures arena write + ring handoff + read-back on same process.
        let handle = arena.allocate_value(i).expect("alloc");
        producer.send(handle).expect("send");
        let got = consumer.try_recv().expect("recv");
        let value = arena.read_copy(got).expect("read");
        assert_eq!(value, i);
    }

    let elapsed = start.elapsed();
    let per_msg_ns = elapsed.as_nanos() / u128::from(messages);
    println!("plexus-loop: {messages} msgs in {elapsed:?}, {per_msg_ns} ns/msg");
}
