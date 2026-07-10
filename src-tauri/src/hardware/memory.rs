use serde::Serialize;
use specta::Type;
use sysinfo::System;

#[derive(Serialize, Type, Clone)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub available_mb: u64,
}

pub fn detect() -> MemoryInfo {
    let mut sys = System::new();
    sys.refresh_memory();

    MemoryInfo {
        total_mb: sys.total_memory() / 1024 / 1024,
        available_mb: sys.available_memory() / 1024 / 1024,
    }
}
