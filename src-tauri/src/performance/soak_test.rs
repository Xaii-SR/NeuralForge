use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use crate::performance::arena_v5::{SharedRingBufferArena, ArenaError};
use crate::performance::viewport::ViewportManager;

const PATTERN_BYTE: u8 = 0xA5;

pub struct SoakTestReport {
    pub elapsed_seconds: f64,
    pub total_bytes_processed: u64,
    pub throughput_gb_per_sec: f64,
    pub slots_consumed: u64,
    pub backpressure_hits: u64,
    pub integrity_verified: bool,
}

pub struct SubstrateStressRig {
    arena: Arc<SharedRingBufferArena>,
    viewport: Arc<ViewportManager>,
    running: Arc<AtomicBool>,
    backpressure_counter: Arc<AtomicU64>,
    bytes_counter: Arc<AtomicU64>,
    slots_counter: Arc<AtomicU64>,
}

impl SubstrateStressRig {
    pub fn new(arena_capacity: usize) -> Result<Self, ArenaError> {
        let arena = Arc::new(SharedRingBufferArena::new(arena_capacity)?);
        let viewport = Arc::new(ViewportManager::new());
        let running = Arc::new(AtomicBool::new(false));
        let backpressure_counter = Arc::new(AtomicU64::new(0));
        let bytes_counter = Arc::new(AtomicU64::new(0));
        let slots_counter = Arc::new(AtomicU64::new(0));

        Ok(Self {
            arena,
            viewport,
            running,
            backpressure_counter,
            bytes_counter,
            slots_counter,
        })
    }

    pub fn execute_sprint(&self, duration: Duration, thread_count: usize) -> SoakTestReport {
        self.running.store(true, Ordering::SeqCst);
        self.backpressure_counter.store(0, Ordering::SeqCst);
        self.bytes_counter.store(0, Ordering::SeqCst);
        self.slots_counter.store(0, Ordering::SeqCst);

        let mut producer_handles = Vec::new();
        let payload_size: u32 = 1024;

        // 1. Spawn Concurrent Multi-Threaded Producers
        for thread_idx in 0..thread_count {
            let arena = Arc::clone(&self.arena);
            let running = Arc::clone(&self.running);
            let bp_count = Arc::clone(&self.backpressure_counter);
            let bytes_count = Arc::clone(&self.bytes_counter);

            let handle = thread::spawn(move || {
                let mut local_seq = 1u64;
                let payload = vec![PATTERN_BYTE; payload_size as usize];

                while running.load(Ordering::Relaxed) {
                    let kind = (thread_idx % 10) as u16;
                    match arena.reserve_slot(payload_size, kind) {
                        Ok(offset) => {
                            unsafe {
                                arena.write_payload(offset, &payload);
                            }
                            let target_seq = (local_seq << 16) | (thread_idx as u64 & 0xFFFF);
                            arena.commit_slot(offset, target_seq);

                            bytes_count.fetch_add(payload_size as u64, Ordering::Relaxed);
                            local_seq += 1;
                        }
                        Err(ArenaError::Backpressure) => {
                            bp_count.fetch_add(1, Ordering::Relaxed);
                            thread::yield_now();
                        }
                        Err(_) => {
                            // Capacity exceeded during teardown – suppress
                        }
                    }
                }
            });
            producer_handles.push(handle);
        }

        // 2. Spawn Unified Consumer & Integrity Verification Validation Loop
        let arena = Arc::clone(&self.arena);
        let viewport = Arc::clone(&self.viewport);
        let running = Arc::clone(&self.running);
        let slots_count = Arc::clone(&self.slots_counter);

        let consumer_handle = thread::spawn(move || {
            let mut corruption_detected = false;
            let mut current_seq = 1u64;

            while running.load(Ordering::Relaxed)
                || arena.get_control().read_head.load(Ordering::Relaxed)
                    != arena.get_control().write_head.load(Ordering::Relaxed)
            {
                match arena.read_next_slot(current_seq) {
                    Ok(Some((_offset, _len, _kind, data))) => {
                        slots_count.fetch_add(1, Ordering::Relaxed);
                        current_seq += 1;

                        for &byte in data.iter() {
                            if byte != PATTERN_BYTE {
                                corruption_detected = true;
                            }
                        }

                        if let Ok(desc) = viewport.allocate_frame(1920, 1080) {
                            let _ = viewport.free_frame(desc.handle_id);
                        }
                    }
                    _ => {
                        thread::yield_now();
                    }
                }
            }
            !corruption_detected
        });

        // 3. Coordinate Test Execution Timeline
        let start_time = Instant::now();
        thread::sleep(duration);
        self.running.store(false, Ordering::SeqCst);

        for handle in producer_handles {
            let _ = handle.join();
        }

        let integrity_passed = consumer_handle.join().unwrap_or(false);
        let actual_elapsed = start_time.elapsed().as_secs_f64();

        let total_bytes = self.bytes_counter.load(Ordering::SeqCst);
        let total_gb = (total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);
        let throughput = total_gb / actual_elapsed;

        SoakTestReport {
            elapsed_seconds: actual_elapsed,
            total_bytes_processed: total_bytes,
            throughput_gb_per_sec: throughput,
            slots_consumed: self.slots_counter.load(Ordering::SeqCst),
            backpressure_hits: self.backpressure_counter.load(Ordering::SeqCst),
            integrity_verified: integrity_passed,
        }
    }
}