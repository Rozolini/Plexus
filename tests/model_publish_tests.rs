#[test]
fn model_publish_consume_ordering() {
    loom::model(|| {
        use loom::sync::Arc;
        use loom::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use loom::thread;

        let prepared = Arc::new(AtomicBool::new(false));
        let committed = Arc::new(AtomicBool::new(false));
        let payload = Arc::new(AtomicU64::new(0));

        let p_prepared = Arc::clone(&prepared);
        let p_committed = Arc::clone(&committed);
        let p_payload = Arc::clone(&payload);

        let producer = thread::spawn(move || {
            p_prepared.store(true, Ordering::Release);
            p_payload.store(42, Ordering::Release);
            p_committed.store(true, Ordering::Release);
        });

        let c_prepared = Arc::clone(&prepared);
        let c_committed = Arc::clone(&committed);
        let c_payload = Arc::clone(&payload);
        let consumer = thread::spawn(move || {
            if c_committed.load(Ordering::Acquire) {
                assert!(c_prepared.load(Ordering::Acquire));
                assert_eq!(c_payload.load(Ordering::Acquire), 42);
            }
        });

        producer.join().expect("join producer");
        consumer.join().expect("join consumer");
    });
}
