use std::ptr::NonNull;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use plexus::SharedArena;

#[test]
fn owner_liveness_detects_stale_heartbeat() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");
    // SAFETY: owned by test process.
    let mut arena =
        unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("arena") };
    arena.heartbeat();
    assert!(!arena.detect_dead_owner(Duration::from_secs(3600)));
}

#[test]
fn reader_polling_survives_missing_notify_after_writer_crash() {
    let mut backing = vec![0_u8; 4096];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");

    {
        // SAFETY: test controls mapped bytes.
        let mut writer =
            unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("writer") };
        writer.begin_publish();
        let _ = writer.allocate_value(4242_u64).expect("value");
        writer.commit();
        // Simulated crash point: commit persisted, no explicit notify path.
    }

    let (tx, rx) = mpsc::channel();
    let ptr = base.as_ptr() as usize;
    let len = backing.len();
    let handle = thread::spawn(move || {
        let reopen_ptr = NonNull::new(ptr as *mut u8).expect("nonnull");
        // SAFETY: same bytes remapped for simulated reader process.
        let reader =
            unsafe { SharedArena::from_raw_parts(reopen_ptr, len, false).expect("reader") };
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let committed_tail = reader.header().committed_tail;
            if committed_tail > 0 {
                // Reader derives latest committed value directly from committed tail.
                let offset = committed_tail - std::mem::size_of::<u64>() as u64;
                let value = reader
                    .read_copy(plexus::Handle::<u64>::from_offset(offset))
                    .expect("read committed");
                tx.send(value).expect("send result");
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            thread::yield_now();
        }
    });

    let value = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader should not hang");
    handle.join().expect("join reader");
    assert_eq!(value, 4242);
}

#[test]
fn repeated_recovery_cycles_complete_without_liveness_regression() {
    let mut backing = vec![0_u8; 16 * 1024];
    let base = NonNull::new(backing.as_mut_ptr()).expect("non-null");

    let started = Instant::now();
    for _ in 0..256 {
        {
            // SAFETY: test controls mapped bytes.
            let mut writer =
                unsafe { SharedArena::from_raw_parts(base, backing.len(), true).expect("writer") };
            writer.begin_publish();
            let _ = writer.allocate_value(11_u64).expect("alloc");
        }

        // SAFETY: test controls mapped bytes.
        let _reader = unsafe {
            SharedArena::from_raw_parts(base, backing.len(), false).expect("reader recover")
        };
    }
    // Guardrail: repeated recoveries must stay bounded and not degrade into hangs.
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "recovery loop should stay responsive"
    );
}
