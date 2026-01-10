use regex::Regex;
use crate::journalctl::JournalEntry;

/// Pre-compiled regex patterns for message normalization
pub struct NormalizeRegex {
    ip: Regex,
    port: Regex,
    pid: Regex,
    container_id: Regex,
}

impl NormalizeRegex {
    pub fn new() -> Self {
        Self {
            ip: Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap(),
            port: Regex::new(r"port \d+").unwrap(),
            pid: Regex::new(r"\[\d+\]").unwrap(),
            container_id: Regex::new(r"[a-f0-9]{12,}").unwrap(),
        }
    }

    /// Normalize message for grouping (remove variable parts)
    pub fn normalize(&self, msg: &str) -> String {
        let msg = self.ip.replace_all(msg, "<IP>");
        let msg = self.port.replace_all(&msg, "port <PORT>");
        let msg = self.pid.replace_all(&msg, "[<PID>]");
        let msg = self.container_id.replace_all(&msg, "<ID>");
        msg.to_string()
    }
}

/// Filter criteria for entries
pub struct FilterCriteria {
    pub unit: Option<String>,
    pub max_priority: u8,
    pub pattern: Option<Regex>,
}

impl FilterCriteria {
    pub fn new(unit: Option<String>, max_priority: u8, pattern: Option<String>) -> anyhow::Result<Self> {
        let pattern = pattern.map(|p| Regex::new(&p)).transpose()?;
        Ok(Self { unit, max_priority, pattern })
    }

    /// Check if entry passes all filters
    pub fn matches(&self, entry: &JournalEntry) -> bool {
        // Filter by priority
        if entry.priority_num() > self.max_priority {
            return false;
        }

        // Filter by unit/service
        if let Some(ref unit_filter) = self.unit {
            let service = entry.service().to_lowercase();
            if !service.contains(&unit_filter.to_lowercase()) {
                return false;
            }
        }

        // Filter by pattern
        if let Some(ref regex) = self.pattern {
            if !regex.is_match(entry.msg()) {
                return false;
            }
        }

        true
    }
}
