use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndexingState {
    Idle,
    Discovering,
    Hashing,
    Parsing,
    Chunking,
    Embedding,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProgress {
    pub workspace_id: String,
    pub state: IndexingState,
    pub files_discovered: u64,
    pub files_processed: u64,
    pub chunks_created: u64,
    pub embeddings_created: u64,
    pub errors: Vec<String>,
}

pub struct WorkspaceService {
    state: Arc<Mutex<HashMap<String, WorkspaceProgress>>>,
}

impl WorkspaceService {
    pub fn new() -> Self {
        Self { state: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn start_scan(&self, workspace_id: &str) {
        let mut map = self.state.lock().unwrap();
        map.insert(workspace_id.to_string(), WorkspaceProgress {
            workspace_id: workspace_id.to_string(),
            state: IndexingState::Discovering,
            files_discovered: 0,
            files_processed: 0,
            chunks_created: 0,
            embeddings_created: 0,
            errors: Vec::new(),
        });
    }

    pub fn get_progress(&self, workspace_id: &str) -> Option<WorkspaceProgress> {
        self.state.lock().unwrap().get(workspace_id).cloned()
    }

    pub fn cancel_scan(&self, workspace_id: &str) {
        self.state.lock().unwrap().remove(workspace_id);
    }
}