use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    enqueue_ok: AtomicU64,
    enqueue_full: AtomicU64,
    dequeue_ok: AtomicU64,
    dequeue_empty: AtomicU64,
    send_latency_total_ns: AtomicU64,
    send_latency_samples: AtomicU64,
    queue_depth_high_watermark: AtomicU64,
    failed_attaches: AtomicU64,
    reclaim_lag_current: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSnapshot {
    pub enqueue_ok: u64,
    pub enqueue_full: u64,
    pub dequeue_ok: u64,
    pub dequeue_empty: u64,
    pub send_latency_avg_ns: u64,
    pub send_latency_samples: u64,
    pub queue_depth_high_watermark: u64,
    pub failed_attaches: u64,
    pub reclaim_lag_current: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertThresholds {
    pub queue_depth_high_watermark: u64,
    pub send_latency_avg_ns: u64,
    pub failed_attaches: u64,
    pub reclaim_lag_current: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    QueueDepthHighWatermark,
    SendLatencyAverage,
    FailedAttaches,
    ReclaimLag,
}

impl MetricSnapshot {
    #[must_use]
    pub fn evaluate_alerts(&self, thresholds: AlertThresholds) -> Vec<AlertKind> {
        let mut alerts = Vec::with_capacity(4);
        if self.queue_depth_high_watermark >= thresholds.queue_depth_high_watermark {
            alerts.push(AlertKind::QueueDepthHighWatermark);
        }
        if self.send_latency_avg_ns >= thresholds.send_latency_avg_ns {
            alerts.push(AlertKind::SendLatencyAverage);
        }
        if self.failed_attaches >= thresholds.failed_attaches {
            alerts.push(AlertKind::FailedAttaches);
        }
        if self.reclaim_lag_current >= thresholds.reclaim_lag_current {
            alerts.push(AlertKind::ReclaimLag);
        }
        alerts
    }
}

impl Metrics {
    pub fn on_enqueue_ok(&self) {
        self.enqueue_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_enqueue_full(&self) {
        self.enqueue_full.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_dequeue_ok(&self) {
        self.dequeue_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_dequeue_empty(&self) {
        self.dequeue_empty.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_send_latency_ns(&self, latency_ns: u64) {
        self.send_latency_total_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
        self.send_latency_samples.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_queue_depth(&self, depth: u64) {
        // Track only monotonic high-water mark; we do not keep full depth history here.
        let _ = self.queue_depth_high_watermark.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |current| (depth > current).then_some(depth),
        );
    }

    pub fn on_failed_attach(&self) {
        self.failed_attaches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_reclaim_lag_current(&self, lag: u64) {
        self.reclaim_lag_current.store(lag, Ordering::Relaxed);
    }

    #[must_use]
    pub fn snapshot(&self) -> MetricSnapshot {
        let samples = self.send_latency_samples.load(Ordering::Relaxed);
        let latency_total = self.send_latency_total_ns.load(Ordering::Relaxed);
        // Average is derived on read to keep hot-path updates as cheap atomic adds.
        let avg = if samples == 0 {
            0
        } else {
            latency_total / samples
        };
        MetricSnapshot {
            enqueue_ok: self.enqueue_ok.load(Ordering::Relaxed),
            enqueue_full: self.enqueue_full.load(Ordering::Relaxed),
            dequeue_ok: self.dequeue_ok.load(Ordering::Relaxed),
            dequeue_empty: self.dequeue_empty.load(Ordering::Relaxed),
            send_latency_avg_ns: avg,
            send_latency_samples: samples,
            queue_depth_high_watermark: self.queue_depth_high_watermark.load(Ordering::Relaxed),
            failed_attaches: self.failed_attaches.load(Ordering::Relaxed),
            reclaim_lag_current: self.reclaim_lag_current.load(Ordering::Relaxed),
        }
    }
}
