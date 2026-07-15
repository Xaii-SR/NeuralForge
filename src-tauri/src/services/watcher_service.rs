use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed(PathBuf, PathBuf),
}

pub struct DebouncedEvents {
    events: Vec<FileEvent>,
    last_flush: std::time::Instant,
    window_ms: u64,
}

impl DebouncedEvents {
    pub fn new(window_ms: u64) -> Self {
        Self { events: Vec::new(), last_flush: std::time::Instant::now(), window_ms }
    }

    pub fn push(&mut self, event: FileEvent) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_flush).as_millis() as u64 > self.window_ms {
            self.events.clear();
            self.last_flush = now;
        }

        // Deduplicate: if the same path is already queued as Modified, skip
        let path = match &event {
            FileEvent::Created(p) | FileEvent::Modified(p) | FileEvent::Removed(p) => p.clone(),
            FileEvent::Renamed(_, _) => { self.events.push(event); return; }
        };

        for existing in &self.events {
            match existing {
                FileEvent::Modified(p) if p == &path => return,
                FileEvent::Created(p) if p == &path => return,
                FileEvent::Removed(p) if p == &path => return,
                _ => {}
            }
        }
        self.events.push(event);
    }

    pub fn drain(&mut self) -> Vec<FileEvent> {
        std::mem::take(&mut self.events)
    }
}

pub struct WatcherService {
    debouncer: Arc<Mutex<DebouncedEvents>>,
}

impl WatcherService {
    pub fn new(debounce_ms: u64) -> Self {
        Self { debouncer: Arc::new(Mutex::new(DebouncedEvents::new(debounce_ms))) }
    }

    pub fn record_event(&self, event: FileEvent) {
        self.debouncer.lock().unwrap().push(event);
    }

    pub fn drain_events(&self) -> Vec<FileEvent> {
        self.debouncer.lock().unwrap().drain()
    }
}