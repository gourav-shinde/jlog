use std::collections::HashMap;
use crate::journalctl::JournalEntry;
use super::filter::NormalizeRegex;

/// Time bucket for time-series data
#[derive(Clone, Default)]
pub struct TimeBucket {
    pub total: usize,
    pub errors: usize,   // priority 0-3
    pub warnings: usize, // priority 4
}

/// Streaming analysis state - accumulates statistics without storing entries
pub struct AnalysisState {
    pub total_entries: usize,
    pub entries_by_priority: [usize; 8],
    pub entries_by_service: HashMap<String, usize>,
    pub error_messages: HashMap<String, usize>,

    // Time-series data (hour buckets)
    pub time_series: HashMap<String, TimeBucket>,

    // Pattern counters
    pub failed_ssh_count: usize,
    pub restart_count: usize,
    pub oom_count: usize,
    pub timeout_count: usize,
    pub disk_issue_count: usize,
    pub firewall_block_count: usize,
}

impl AnalysisState {
    pub fn new() -> Self {
        Self {
            total_entries: 0,
            entries_by_priority: [0; 8],
            entries_by_service: HashMap::new(),
            error_messages: HashMap::new(),
            time_series: HashMap::new(),
            failed_ssh_count: 0,
            restart_count: 0,
            oom_count: 0,
            timeout_count: 0,
            disk_issue_count: 0,
            firewall_block_count: 0,
        }
    }

    /// Process a single entry - called for each line during streaming
    pub fn process_entry(&mut self, entry: &JournalEntry, normalize_regex: &NormalizeRegex) {
        self.total_entries += 1;

        let priority = entry.priority_num() as usize;
        if priority < 8 {
            self.entries_by_priority[priority] += 1;
        }

        // Count by service
        *self.entries_by_service.entry(entry.service()).or_insert(0) += 1;

        // Track time-series data
        if let Some(bucket_key) = entry.hour_bucket() {
            let bucket = self.time_series.entry(bucket_key).or_default();
            bucket.total += 1;
            if priority <= 3 {
                bucket.errors += 1;
            } else if priority == 4 {
                bucket.warnings += 1;
            }
        }

        // Track error messages (priority <= 4: warning and above)
        if priority <= 4 {
            let msg = normalize_regex.normalize(entry.msg());
            if !msg.is_empty() {
                *self.error_messages.entry(msg).or_insert(0) += 1;
            }
        }

        // Detect patterns inline
        self.detect_patterns(entry);
    }

    fn detect_patterns(&mut self, entry: &JournalEntry) {
        let msg = entry.msg();
        let msg_lower = msg.to_lowercase();

        if msg.contains("Failed password") {
            self.failed_ssh_count += 1;
        }
        if msg.contains("restart") || msg.contains("Restarting") {
            self.restart_count += 1;
        }
        if msg.contains("Out of memory") || msg.contains("OOM") {
            self.oom_count += 1;
        }
        if msg_lower.contains("timeout") {
            self.timeout_count += 1;
        }
        if msg_lower.contains("disk") && (msg_lower.contains("error") || msg_lower.contains("full") || msg_lower.contains("90%")) {
            self.disk_issue_count += 1;
        }
        if msg.contains("UFW BLOCK") || msg.contains("BLOCKED") {
            self.firewall_block_count += 1;
        }
    }

    /// Get top N error messages sorted by count
    pub fn top_errors(&self, n: usize) -> Vec<(String, usize)> {
        let mut errors: Vec<_> = self.error_messages.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        errors.sort_by(|a, b| b.1.cmp(&a.1));
        errors.truncate(n);
        errors
    }

    /// Get top N services sorted by count
    pub fn top_services(&self, n: usize) -> Vec<(String, usize)> {
        let mut services: Vec<_> = self.entries_by_service.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        services.sort_by(|a, b| b.1.cmp(&a.1));
        services.truncate(n);
        services
    }

    /// Get time-series data sorted by time
    pub fn sorted_time_series(&self) -> Vec<(String, TimeBucket)> {
        let mut series: Vec<_> = self.time_series.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        series.sort_by(|a, b| a.0.cmp(&b.0));
        series
    }

    /// Get detected patterns for reporting
    pub fn get_patterns(&self) -> Vec<PatternInfo> {
        let mut patterns = Vec::new();

        if self.failed_ssh_count >= 3 {
            patterns.push(PatternInfo {
                name: "SSH Brute Force Attempt",
                description: format!("{} failed password attempts", self.failed_ssh_count),
                severity: if self.failed_ssh_count >= 10 { Severity::Critical } else { Severity::Warning },
                count: self.failed_ssh_count,
            });
        }
        if self.oom_count > 0 {
            patterns.push(PatternInfo {
                name: "Out of Memory",
                description: format!("{} OOM killer events", self.oom_count),
                severity: Severity::Critical,
                count: self.oom_count,
            });
        }
        if self.restart_count >= 2 {
            patterns.push(PatternInfo {
                name: "Service Restarts",
                description: format!("{} restart events", self.restart_count),
                severity: Severity::Warning,
                count: self.restart_count,
            });
        }
        if self.timeout_count >= 2 {
            patterns.push(PatternInfo {
                name: "Connection Timeouts",
                description: format!("{} timeout events", self.timeout_count),
                severity: Severity::Warning,
                count: self.timeout_count,
            });
        }
        if self.disk_issue_count > 0 {
            patterns.push(PatternInfo {
                name: "Disk Issues",
                description: format!("{} disk-related issues", self.disk_issue_count),
                severity: Severity::Warning,
                count: self.disk_issue_count,
            });
        }
        if self.firewall_block_count >= 2 {
            patterns.push(PatternInfo {
                name: "Firewall Blocks",
                description: format!("{} blocked connections", self.firewall_block_count),
                severity: Severity::Info,
                count: self.firewall_block_count,
            });
        }

        patterns
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

pub struct PatternInfo {
    pub name: &'static str,
    pub description: String,
    pub severity: Severity,
    pub count: usize,
}
