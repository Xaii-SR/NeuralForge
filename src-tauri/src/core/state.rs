use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub struct AppState {
    pub workspace_root: Mutex<Option<PathBuf>>,
    pub workspace_activation: Mutex<()>,
    pub filesystem_mutation: Mutex<()>,
    workspace_generation: AtomicU64,
    workspace_open_sequence: AtomicU64,
    latest_workspace_open: AtomicU64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workspace_root: Mutex::new(None),
            workspace_activation: Mutex::new(()),
            filesystem_mutation: Mutex::new(()),
            workspace_generation: AtomicU64::new(0),
            workspace_open_sequence: AtomicU64::new(0),
            latest_workspace_open: AtomicU64::new(0),
        }
    }
}

impl AppState {
    pub fn begin_workspace_open(&self) -> u64 {
        let request = self.workspace_open_sequence.fetch_add(1, Ordering::SeqCst) + 1;
        self.latest_workspace_open.store(request, Ordering::SeqCst);
        request
    }

    pub fn is_latest_workspace_open(&self, request: u64) -> bool {
        self.latest_workspace_open.load(Ordering::SeqCst) == request
    }

    pub fn advance_workspace_generation(&self) -> u64 {
        self.workspace_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn workspace_generation(&self) -> u64 {
        self.workspace_generation.load(Ordering::SeqCst)
    }

    pub fn matches_workspace_generation(&self, expected: u64) -> bool {
        expected != 0 && self.workspace_generation() == expected
    }
}
