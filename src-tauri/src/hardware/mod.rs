pub mod cpu;
pub mod gpu;
pub mod memory;

use cpu::CpuInfo;
use gpu::GpuInfo;
use memory::MemoryInfo;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
}

pub fn detect_all() -> HardwareInfo {
    HardwareInfo {
        cpu: cpu::detect(),
        memory: memory::detect(),
        gpus: gpu::detect(),
    }
}

#[tauri::command]
pub fn get_hardware_info() -> HardwareInfo {
    detect_all()
}
