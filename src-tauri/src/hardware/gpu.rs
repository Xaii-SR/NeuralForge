use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub vram_mb: u64,
    pub utilization_percent: Option<f32>,
}

fn vendor_name(vendor_id: u32) -> String {
    match vendor_id {
        0x10DE => "NVIDIA".to_string(),
        0x1002 => "AMD".to_string(),
        0x8086 => "Intel".to_string(),
        _ => format!("Unknown (0x{vendor_id:04X})"),
    }
}

#[cfg(windows)]
pub fn detect() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut gpus = Vec::new();

    unsafe {
        let factory: windows::core::Result<IDXGIFactory1> = CreateDXGIFactory1();
        let Ok(factory) = factory else {
            return gpus;
        };

        let mut index = 0u32;
        loop {
            let adapter = factory.EnumAdapters1(index);
            index += 1;
            let Ok(adapter) = adapter else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };

            // Skip the Microsoft Basic Render Driver (software rasterizer, vendor id 0x1414)
            if desc.VendorId == 0x1414 {
                continue;
            }

            let name_end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..name_end]);

            gpus.push(GpuInfo {
                name,
                vendor: vendor_name(desc.VendorId),
                vram_mb: (desc.DedicatedVideoMemory as u64) / 1024 / 1024,
                utilization_percent: None,
            });
        }
    }

    gpus
}

#[cfg(not(windows))]
pub fn detect() -> Vec<GpuInfo> {
    Vec::new()
}
