use std::collections::BTreeSet;

pub struct LogEntry {
    pub line_num: usize,
    pub timestamp: String,
    pub priority: u8,
    pub service: String,
    pub message: String,
}

pub struct LogStore {
    pub entries: Vec<LogEntry>,
    pub services: BTreeSet<String>,
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            services: BTreeSet::new(),
        }
    }

    pub fn service_names(&self) -> Vec<String> {
        self.services.iter().cloned().collect()
    }
}
