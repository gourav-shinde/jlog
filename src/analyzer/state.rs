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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_is_empty() {
        let store = LogStore::new();
        assert!(store.entries.is_empty());
        assert!(store.services.is_empty());
    }

    #[test]
    fn service_names_sorted() {
        let mut store = LogStore::new();
        store.services.insert("sshd".to_string());
        store.services.insert("kernel".to_string());
        store.services.insert("nginx".to_string());
        let names = store.service_names();
        assert_eq!(names, vec!["kernel", "nginx", "sshd"]);
    }

    #[test]
    fn service_names_empty() {
        let store = LogStore::new();
        assert!(store.service_names().is_empty());
    }
}
