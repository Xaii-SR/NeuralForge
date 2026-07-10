use serde::Serialize;
use specta::Type;

#[derive(Serialize, Type, Clone)]
pub struct VramCheckResult {
    pub sufficient: bool,
    pub required_mb: u64,
    pub available_mb: u64,
    pub message: String,
}

/// Rough VRAM estimate from parameter count + quantization. Not a precise
/// model of KV-cache/runtime overhead - a 20% margin is added on top of raw
/// weight size, which is conservative enough to catch "this obviously won't
/// fit" cases without needing per-architecture profiling (that's Phase 4's
/// benchmark system).
pub fn estimate_required_mb(parameter_size: &str, quantization_level: &str) -> u64 {
    let params_billions: f64 = parameter_size.trim_end_matches(['B', 'b']).parse().unwrap_or(0.0);

    let bytes_per_param = if quantization_level.starts_with("Q4") {
        0.5
    } else if quantization_level.starts_with("Q5") {
        0.625
    } else if quantization_level.starts_with("Q6") {
        0.75
    } else if quantization_level.starts_with("Q8") {
        1.0
    } else if quantization_level.starts_with("F16") || quantization_level.starts_with("FP16") {
        2.0
    } else if quantization_level.starts_with("F32") || quantization_level.starts_with("FP32") {
        4.0
    } else {
        0.6
    };

    let model_bytes = params_billions * 1_000_000_000.0 * bytes_per_param;
    let with_overhead = model_bytes * 1.2;
    (with_overhead / 1024.0 / 1024.0) as u64
}

pub fn check(parameter_size: &str, quantization_level: &str, hardware: &crate::hardware::HardwareInfo) -> VramCheckResult {
    let required_mb = estimate_required_mb(parameter_size, quantization_level);

    let gpu_vram_mb = hardware.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0);
    let available_mb = if gpu_vram_mb > 0 { gpu_vram_mb } else { hardware.memory.available_mb };
    let source = if gpu_vram_mb > 0 { "GPU VRAM" } else { "system RAM (no dedicated GPU detected)" };

    let sufficient = available_mb >= required_mb;
    let message = if sufficient {
        format!("{required_mb}MB required, {available_mb}MB available via {source} - OK")
    } else {
        format!(
            "{required_mb}MB required, only {available_mb}MB available via {source} - insufficient, try a smaller or more quantized model"
        )
    };

    VramCheckResult {
        sufficient,
        required_mb,
        available_mb,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_scales_with_params_and_quant() {
        let q4_1b = estimate_required_mb("1B", "Q4_0");
        let q4_14b = estimate_required_mb("14.8B", "Q4_K_M");
        assert!(q4_14b > q4_1b * 10);

        let f16_1b = estimate_required_mb("1B", "F16");
        assert!(f16_1b > q4_1b);
    }

    #[test]
    fn check_rejects_when_insufficient() {
        let hardware = crate::hardware::HardwareInfo {
            cpu: crate::hardware::cpu::detect(),
            memory: crate::hardware::memory::MemoryInfo { total_mb: 8000, available_mb: 500 },
            gpus: vec![],
        };
        let result = check("70B", "F16", &hardware);
        assert!(!result.sufficient);
    }

    #[test]
    fn check_accepts_when_sufficient() {
        let hardware = crate::hardware::HardwareInfo {
            cpu: crate::hardware::cpu::detect(),
            memory: crate::hardware::memory::MemoryInfo { total_mb: 32000, available_mb: 16000 },
            gpus: vec![],
        };
        let result = check("1B", "Q4_0", &hardware);
        assert!(result.sufficient);
    }
}
