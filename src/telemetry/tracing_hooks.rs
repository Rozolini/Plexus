use crate::telemetry::MetricSnapshot;

pub fn emit_endpoint_snapshot(name: &str, snapshot: MetricSnapshot) {
    tracing::info!(
        target: "plexus.ipc",
        endpoint = name,
        enqueue_ok = snapshot.enqueue_ok,
        enqueue_full = snapshot.enqueue_full,
        dequeue_ok = snapshot.dequeue_ok,
        dequeue_empty = snapshot.dequeue_empty,
        send_latency_avg_ns = snapshot.send_latency_avg_ns,
        send_latency_samples = snapshot.send_latency_samples,
        queue_depth_high_watermark = snapshot.queue_depth_high_watermark,
        "plexus endpoint metric snapshot"
    );
}
