use plexus::Metrics;

fn main() {
    let metrics = Metrics::default();
    let snapshot = metrics.snapshot();
    // Keep output as a single JSON line so it can be consumed by simple health probes.
    println!(
        "{{\"status\":\"ok\",\"enqueue_ok\":{},\"dequeue_ok\":{},\"failed_attaches\":{}}}",
        snapshot.enqueue_ok, snapshot.dequeue_ok, snapshot.failed_attaches
    );
}
