use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_LATENCY_SAMPLES: usize = 20;
const FAILURE_THRESHOLD_FOR_COOLDOWN: u32 = 3;
const COOLDOWN_DURATION: Duration = Duration::from_secs(30);

struct ProviderStats {
    latencies_ms: Vec<f64>,
    failure_count: u32,
    cooldown_until: Option<Instant>,
}

impl Default for ProviderStats {
    fn default() -> Self {
        Self {
            latencies_ms: Vec::new(),
            failure_count: 0,
            cooldown_until: None,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ProviderHealthInfo {
    pub provider: String,
    pub healthy: bool,
    pub avg_latency_ms: Option<f64>,
    pub failure_count: u32,
    pub cooldown_seconds_remaining: Option<u64>,
}

#[derive(Default)]
pub struct HealthRegistry {
    stats: Mutex<HashMap<String, ProviderStats>>,
}

impl HealthRegistry {
    pub fn record_success(&self, provider: &str, latency_ms: f64) {
        let mut stats = self.stats.lock().unwrap();
        let entry = stats.entry(provider.to_string()).or_default();
        entry.latencies_ms.push(latency_ms);
        if entry.latencies_ms.len() > MAX_LATENCY_SAMPLES {
            entry.latencies_ms.remove(0);
        }
        entry.failure_count = 0;
        entry.cooldown_until = None;
    }

    pub fn record_failure(&self, provider: &str) {
        let mut stats = self.stats.lock().unwrap();
        let entry = stats.entry(provider.to_string()).or_default();
        entry.failure_count += 1;
        if entry.failure_count >= FAILURE_THRESHOLD_FOR_COOLDOWN {
            entry.cooldown_until = Some(Instant::now() + COOLDOWN_DURATION);
            tracing::warn!(target: "ai", event = "provider_degraded", provider, failure_count = entry.failure_count);
        }
    }

    pub fn is_healthy(&self, provider: &str) -> bool {
        let stats = self.stats.lock().unwrap();
        match stats.get(provider).and_then(|s| s.cooldown_until) {
            Some(until) => Instant::now() >= until,
            None => true,
        }
    }

    pub fn snapshot(&self) -> Vec<ProviderHealthInfo> {
        let stats = self.stats.lock().unwrap();
        let now = Instant::now();
        stats
            .iter()
            .map(|(provider, s)| {
                let avg_latency_ms = if s.latencies_ms.is_empty() {
                    None
                } else {
                    Some(s.latencies_ms.iter().sum::<f64>() / s.latencies_ms.len() as f64)
                };
                let (healthy, cooldown_seconds_remaining) = match s.cooldown_until {
                    Some(until) if until > now => (false, Some((until - now).as_secs())),
                    _ => (true, None),
                };
                ProviderHealthInfo {
                    provider: provider.clone(),
                    healthy,
                    avg_latency_ms,
                    failure_count: s.failure_count,
                    cooldown_seconds_remaining,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_by_default_for_unknown_provider() {
        let registry = HealthRegistry::default();
        assert!(registry.is_healthy("ollama"));
    }

    #[test]
    fn degrades_after_threshold_failures() {
        let registry = HealthRegistry::default();
        for _ in 0..FAILURE_THRESHOLD_FOR_COOLDOWN {
            registry.record_failure("ollama");
        }
        assert!(!registry.is_healthy("ollama"));
    }

    #[test]
    fn success_resets_failure_count_and_cooldown() {
        let registry = HealthRegistry::default();
        for _ in 0..FAILURE_THRESHOLD_FOR_COOLDOWN {
            registry.record_failure("ollama");
        }
        assert!(!registry.is_healthy("ollama"));

        registry.record_success("ollama", 42.0);
        assert!(registry.is_healthy("ollama"));

        let snapshot = registry.snapshot();
        let entry = snapshot.iter().find(|s| s.provider == "ollama").unwrap();
        assert_eq!(entry.failure_count, 0);
        assert_eq!(entry.avg_latency_ms, Some(42.0));
    }
}
