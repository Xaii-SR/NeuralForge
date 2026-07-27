use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::errors::{AppError, AppResult};

#[derive(Default)]
pub struct RequestRegistry {
    requests: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl RequestRegistry {
    pub fn begin(&self, request_id: &str) -> Result<Arc<AtomicBool>, String> {
        let mut requests = self.requests.lock().map_err(|error| error.to_string())?;
        if requests.contains_key(request_id) {
            return Err("request id is already active".to_string());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        requests.insert(request_id.to_string(), cancelled.clone());
        Ok(cancelled)
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let Ok(requests) = self.requests.lock() else {
            return false;
        };
        let Some(cancelled) = requests.get(request_id) else {
            return false;
        };
        cancelled.store(true, Ordering::Release);
        true
    }

    pub fn finish(&self, request_id: &str) {
        if let Ok(mut requests) = self.requests.lock() {
            requests.remove(request_id);
        }
    }
}

pub fn is_cancelled(cancelled: &AtomicBool) -> bool {
    cancelled.load(Ordering::Acquire)
}

pub async fn run_cancellable<T, F>(cancelled: &AtomicBool, future: F) -> AppResult<T>
where
    F: Future<Output = AppResult<T>>,
{
    if is_cancelled(cancelled) {
        return Err(AppError::CommandRejected("request cancelled".to_string()));
    }

    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result,
            _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {
                if is_cancelled(cancelled) {
                    return Err(AppError::CommandRejected("request cancelled".to_string()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_unique_cancellable_and_reusable_after_finish() {
        let registry = RequestRegistry::default();
        let first = registry.begin("request-1").unwrap();
        assert!(registry.begin("request-1").is_err());
        assert!(registry.cancel("request-1"));
        assert!(is_cancelled(&first));
        registry.finish("request-1");
        assert!(registry.begin("request-1").is_ok());
    }

    #[tokio::test]
    async fn cancellation_drops_an_in_flight_future() {
        let registry = RequestRegistry::default();
        let cancelled = registry.begin("request-2").unwrap();
        registry.cancel("request-2");
        let result = run_cancellable(&cancelled, async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok(())
        })
        .await;
        assert!(matches!(result, Err(AppError::CommandRejected(_))));
    }

    #[tokio::test]
    async fn finish_clears_for_new_begin() {
        let registry = RequestRegistry::default();
        registry.begin("r-1").unwrap();
        registry.finish("r-1");
        let second = registry.begin("r-1").unwrap();
        registry.cancel("r-1");
        assert!(is_cancelled(&second));
    }

    #[tokio::test]
    async fn run_cancellable_returns_future_success_when_not_cancelled() {
        let registry = RequestRegistry::default();
        let cancelled = registry.begin("r-success").unwrap();
        let result = run_cancellable(&cancelled, async { Ok::<_, AppError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn run_cancellable_returns_error_if_already_cancelled() {
        let registry = RequestRegistry::default();
        let cancelled = registry.begin("r-already-cancelled").unwrap();
        registry.cancel("r-already-cancelled");
        let result = run_cancellable(&cancelled, async { Ok::<_, AppError>(1) }).await;
        assert!(matches!(result, Err(AppError::CommandRejected(_))));
    }

    #[test]
    fn cancel_on_unknown_request_id_is_noop() {
        let registry = RequestRegistry::default();
        assert!(!registry.cancel("nonexistent"));
    }
}
