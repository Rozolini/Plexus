use std::ptr::NonNull;

use plexus::telemetry::{AlertKind, AlertThresholds};
use plexus::{SharedArena, SharedEndpoint};

#[test]
fn alert_simulation_threshold_breaches_are_detected() {
    let mut backing = vec![0_u8; 1024 * 32];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: test owns the memory for arena lifetime.
    let arena = unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    let endpoint = SharedEndpoint::<u64>::new(arena, 64).expect("endpoint");
    for i in 0..16_u64 {
        endpoint.send_copy(i).expect("send");
    }
    endpoint.metrics().on_failed_attach();
    endpoint.metrics().set_reclaim_lag_current(120);

    let snapshot = endpoint.metrics().snapshot();
    let alerts = snapshot.evaluate_alerts(AlertThresholds {
        queue_depth_high_watermark: 8,
        send_latency_avg_ns: 0,
        failed_attaches: 1,
        reclaim_lag_current: 100,
    });

    assert!(alerts.contains(&AlertKind::QueueDepthHighWatermark));
    assert!(alerts.contains(&AlertKind::SendLatencyAverage));
    assert!(alerts.contains(&AlertKind::FailedAttaches));
    assert!(alerts.contains(&AlertKind::ReclaimLag));
}
