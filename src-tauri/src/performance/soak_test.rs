use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use crate::performance::arena_v5::{SharedRingBufferArena, ArenaError, SlotHeader, FLAG_COMMITTED, WRAP_SENTINEL};

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
    running: Arc<AtomicBool>,
    backpressure_counter: Arc<AtomicU64>,
    bytes_counter: Arc<AtomicU64>,
    slots_counter: Arc<AtomicU64>,
}

impl SubstrateStressRig {
    pub fn new(arena_capacity: usize) -> Result<Self, ArenaError> {
        let arena = Arc::new(SharedRingBufferArena::new(arena_capacity)?);
        Ok(Self {
            arena,
            running: Arc::new(AtomicBool::new(false)),
            backpressure_counter: Arc::new(AtomicU64::new(0)),
            bytes_counter: Arc::new(AtomicU64::new(0)),
            slots_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn execute_sprint(&self, duration: Duration, thread_count: usize) -> SoakTestReport {
        self.running.store(true, Ordering::SeqCst);
        self.backpressure_counter.store(0, Ordering::SeqCst);
        self.bytes_counter.store(0, Ordering::SeqCst);
        self.slots_counter.store(0, Ordering::SeqCst);

        let mut producer_handles = Vec::new();
        let payload_size: u32 = 1024;

        for _thread_idx in 0..thread_count {
            let arena = Arc::clone(&self.arena);
            let running = Arc::clone(&self.running);
            let bp_count = Arc::clone(&self.backpressure_counter);
            let bytes_count = Arc::clone(&self.bytes_counter);

            let handle = thread::spawn(move || {
                let payload = vec![PATTERN_BYTE; payload_size as usize];
                while running.load(Ordering::Relaxed) {
                    match arena.reserve_slot(payload_size, 0) {
                        Ok(offset) => {
                            unsafe { arena.write_payload(offset, &payload); }
                            arena.commit_slot(offset, 0);
                            bytes_count.fetch_add(payload_size as u64, Ordering::Relaxed);
                        }
                        Err(ArenaError::Backpressure) => {
                            bp_count.fetch_add(1, Ordering::Relaxed);
                            thread::yield_now();
                        }
                        Err(_) => {}
                    }
                }
            });
            producer_handles.push(handle);
        }

        let arena = Arc::clone(&self.arena);
        let running = Arc::clone(&self.running);
        let slots_count = Arc::clone(&self.slots_counter);

        let consumer_handle = thread::spawn(move || {
            let mut corruption_detected = false;
            let header_size = std::mem::size_of::<SlotHeader>();
            let ctrl_size = std::mem::size_of::<crate::performance::arena_v5::ArenaControl>() as u64;

            while running.load(Ordering::Relaxed)
                || arena.get_control().read_head.load(Ordering::Relaxed)
                    != arena.get_control().write_head.load(Ordering::Relaxed)
            {
                let read_pos = arena.get_control().read_head.load(Ordering::Acquire);
                let write_pos = arena.get_control().write_head.load(Ordering::Acquire);
                if read_pos == write_pos { thread::yield_now(); continue; }

                unsafe {
                    let header_ptr = (arena.get_control() as *const _ as *const u8)
                        .add(read_pos as usize) as *const SlotHeader;
                    if (*header_ptr).flags & FLAG_COMMITTED == 0 {
                        thread::yield_now(); continue;
                    }
                    let len = (*header_ptr).len;
                    if len == WRAP_SENTINEL {
                        arena.get_control().read_head.store(ctrl_size, Ordering::Release);
                        continue;
                    }

                    let payload_size = (len as usize + 7) & !7;
                    let data_ptr = (arena.get_control() as *const _ as *const u8)
                        .add(read_pos as usize + header_size);
                    let data = std::slice::from_raw_parts(data_ptr, len as usize);

                    slots_count.fetch_add(1, Ordering::Relaxed);
                    for &byte in data.iter() {
                        if byte != PATTERN_BYTE { corruption_detected = true; }
                    }

                    let next_read = read_pos + (header_size + payload_size) as u64;
                    arena.get_control().read_head.store(next_read, Ordering::Release);
                }
            }
            !corruption_detected
        });

        let start_time = Instant::now();
        thread::sleep(duration);
        self.running.store(false, Ordering::SeqCst);

        for handle in producer_handles { let _ = handle.join(); }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_substrate_throughput_telemetry() {
        let capacity = 16 * 1024 * 1024;
        let rig = SubstrateStressRig::new(capacity).expect("Failed to initialize Substrate Stress Rig");

        println!("==================================================");
        println!("NEURAL FORGE SUBSTRATE SOAK TEST INITIATED");
        println!("Capacity: 16 MB | Threads: 4 | Duration: 3 seconds");
        println!("==================================================");

        let report = rig.execute_sprint(Duration::from_secs(3), 4);

        println!("SOAK TEST RESULTS:");
        println!("Elapsed Time:        {:.2} seconds", report.elapsed_seconds);
        println!("Total Processed:     {} bytes", report.total_bytes_processed);
        println!("Throughput:          {:.4} GB/s", report.throughput_gb_per_sec);
        println!("Slots Consumed:      {}", report.slots_consumed);
        println!("Backpressure Hits:   {}", report.backpressure_hits);
        println!("Integrity Verified:  {}", report.integrity_verified);
        println!("==================================================");

        assert!(report.integrity_verified);
        assert!(report.total_bytes_processed > 0);
    }
}