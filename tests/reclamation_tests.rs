use plexus::EpochManager;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;

#[test]
fn retired_records_are_collected_without_readers() {
    let manager = EpochManager::new();
    manager.retire_offset(11);
    manager.advance_epoch();
    manager.retire_offset(22);
    manager.advance_epoch();

    let reclaimed = manager.try_collect();
    assert_eq!(reclaimed.len(), 2);
}

#[test]
fn retired_records_wait_for_reader_guards() {
    let manager = EpochManager::new();
    let _guard = manager.pin();
    manager.retire_offset(44);
    manager.advance_epoch();
    assert!(manager.try_collect().is_empty());
}

#[test]
fn no_use_after_free_in_reclamation_stress() {
    let manager = Arc::new(EpochManager::new());
    let retired_count = Arc::new(AtomicU64::new(0));
    let writers_done = Arc::new(AtomicUsize::new(0));
    let stop_collector = Arc::new(AtomicBool::new(false));
    let reclaimed_offsets = Arc::new(Mutex::new(Vec::<u64>::new()));

    let mut readers = Vec::new();
    for _ in 0..4 {
        let manager = Arc::clone(&manager);
        readers.push(thread::spawn(move || {
            for _ in 0..4000 {
                {
                    let _guard = manager.pin();
                    std::hint::spin_loop();
                }
            }
        }));
    }

    let mut writers = Vec::new();
    for writer_id in 0..3_u64 {
        let manager = Arc::clone(&manager);
        let retired_count = Arc::clone(&retired_count);
        let writers_done = Arc::clone(&writers_done);
        writers.push(thread::spawn(move || {
            for i in 0..5000_u64 {
                let offset = writer_id * 1_000_000 + i;
                manager.retire_offset(offset);
                retired_count.fetch_add(1, Ordering::AcqRel);
                if i % 2 == 0 {
                    manager.advance_epoch();
                }
            }
            writers_done.fetch_add(1, Ordering::AcqRel);
        }));
    }

    let collector = {
        let manager = Arc::clone(&manager);
        let writers_done = Arc::clone(&writers_done);
        let stop_collector = Arc::clone(&stop_collector);
        let reclaimed_offsets = Arc::clone(&reclaimed_offsets);
        thread::spawn(move || {
            loop {
                let reclaimed = manager.try_collect();
                if !reclaimed.is_empty() {
                    let mut out = reclaimed_offsets.lock().expect("collector lock");
                    out.extend(reclaimed.into_iter().map(|record| record.handle_offset));
                }

                if writers_done.load(Ordering::Acquire) == 3
                    && stop_collector.load(Ordering::Acquire)
                {
                    break;
                }
                thread::yield_now();
            }
        })
    };

    for t in writers {
        t.join().expect("writer join");
    }
    for t in readers {
        t.join().expect("reader join");
    }

    stop_collector.store(true, Ordering::Release);
    // Push epoch forward twice so all retire records become collectible after readers exit.
    manager.advance_epoch();
    manager.advance_epoch();
    {
        let mut out = reclaimed_offsets.lock().expect("collector lock");
        out.extend(
            manager
                .try_collect()
                .into_iter()
                .map(|record| record.handle_offset),
        );
    }
    collector.join().expect("collector join");

    let retired_total = retired_count.load(Ordering::Acquire);
    let mut reclaimed = reclaimed_offsets.lock().expect("final lock").clone();
    reclaimed.sort_unstable();
    reclaimed.dedup();
    assert_eq!(
        reclaimed.len() as u64,
        retired_total,
        "every retired record must be reclaimed once"
    );
}
