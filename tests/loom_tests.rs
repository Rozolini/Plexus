#[test]
fn loom_smoke_atomic_ordering() {
    loom::model(|| {
        use loom::sync::Arc;
        use loom::sync::atomic::{AtomicBool, Ordering};
        use loom::thread;

        let flag = Arc::new(AtomicBool::new(false));
        let writer_flag = Arc::clone(&flag);

        let writer = thread::spawn(move || {
            writer_flag.store(true, Ordering::Release);
        });

        let reader = thread::spawn(move || {
            let _seen = flag.load(Ordering::Acquire);
        });

        writer.join().expect("writer join");
        reader.join().expect("reader join");
    });
}
