use serde::Serialize;
use sysinfo::System;

#[derive(Serialize, Clone)]
pub struct CpuInfo {
    pub brand: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub frequency_mhz: u64,
}

pub fn detect() -> CpuInfo {
    let mut sys = System::new();
    sys.refresh_cpu_all();

    let cpus = sys.cpus();
    let brand = cpus
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let frequency_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let logical_cores = cpus.len();
    let physical_cores = sys.physical_core_count().unwrap_or(logical_cores);

    CpuInfo {
        brand,
        physical_cores,
        logical_cores,
        frequency_mhz,
    }
}
