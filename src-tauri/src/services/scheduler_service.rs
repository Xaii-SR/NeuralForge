use std::collections::BinaryHeap;
use std::cmp::Ordering;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobPriority {
    Critical,
    High,
    Medium,
    Low,
}

impl Ord for JobPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        let v = |p: &JobPriority| match p {
            JobPriority::Critical => 4,
            JobPriority::High => 3,
            JobPriority::Medium => 2,
            JobPriority::Low => 1,
        };
        v(self).cmp(&v(other))
    }
}

impl PartialOrd for JobPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexingAction {
    HashFile,
    ParseFile,
    ChunkFile,
    EmbedChunks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexJob {
    pub id: String,
    pub workspace_id: String,
    pub path: String,
    pub action: IndexingAction,
    pub priority: JobPriority,
    pub created_at: i64,
}

impl Ord for IndexJob {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

impl PartialOrd for IndexJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for IndexJob {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for IndexJob {}

pub struct SchedulerService {
    queue: BinaryHeap<IndexJob>,
    max_capacity: usize,
}

impl SchedulerService {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            queue: BinaryHeap::new(),
            max_capacity,
        }
    }

    pub fn submit(&mut self, job: IndexJob) {
        if self.queue.len() >= self.max_capacity {
            let mut low_priority_idx = None;
            let jobs: Vec<_> = self.queue.iter().collect();
            for (i, j) in jobs.iter().enumerate() {
                if j.priority == JobPriority::Low {
                    low_priority_idx = Some(i);
                    break;
                }
            }
            if let Some(_) = low_priority_idx {
                let mut temp = Vec::new();
                while let Some(j) = self.queue.pop() {
                    if j.priority != JobPriority::Low {
                        temp.push(j);
                    } else {
                        break;
                    }
                }
                for j in temp { self.queue.push(j); }
            } else {
                return;
            }
        }
        self.queue.push(job);
    }

    pub fn next(&mut self) -> Option<IndexJob> {
        self.queue.pop()
    }

    pub fn size(&self) -> usize {
        self.queue.len()
    }
}