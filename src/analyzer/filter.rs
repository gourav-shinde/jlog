use regex::Regex;
use once_cell::sync::Lazy;
use crate::journalctl::JournalEntry;

/// Pre-compiled regex patterns for aggressive message normalization
/// This groups similar messages together by replacing variable parts
static NORMALIZE_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        // UUIDs (must be before hex to avoid partial matches)
        (Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}").unwrap(), "<UUID>"),
        // MAC addresses
        (Regex::new(r"([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}").unwrap(), "<MAC>"),
        // IPv6 addresses (simplified)
        (Regex::new(r"[0-9a-fA-F:]{15,}").unwrap(), "<IPv6>"),
        // IPv4 addresses
        (Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap(), "<IP>"),
        // Timestamps with microseconds (2024-01-15T14:30:00.123456)
        (Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?Z?").unwrap(), "<TIME>"),
        // Time only (14:30:00 or 14:30:00.123)
        (Regex::new(r"\b\d{2}:\d{2}:\d{2}(\.\d+)?\b").unwrap(), "<TIME>"),
        // Date formats (2024-01-15, 01/15/2024, Jan 15)
        (Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap(), "<DATE>"),
        (Regex::new(r"\b\d{2}/\d{2}/\d{4}\b").unwrap(), "<DATE>"),
        // Container/Docker IDs (12+ hex chars)
        (Regex::new(r"\b[0-9a-f]{12,64}\b").unwrap(), "<ID>"),
        // Hex values (0x prefix or standalone 8+ hex)
        (Regex::new(r"0x[0-9a-fA-F]+").unwrap(), "<HEX>"),
        (Regex::new(r"\b[0-9a-fA-F]{8,}\b").unwrap(), "<HEX>"),
        // File paths (Unix style) - require at least 2 components
        (Regex::new(r"/[\w.\-]+(/[\w.\-]+)+/?").unwrap(), "<PATH>"),
        // URLs
        (Regex::new(r"https?://[^\s]+").unwrap(), "<URL>"),
        // Email addresses
        (Regex::new(r"[\w.\-]+@[\w.\-]+\.\w+").unwrap(), "<EMAIL>"),
        // Memory sizes (123KB, 45.6MB, 1.2GiB)
        (Regex::new(r"\b\d+(\.\d+)?\s*(B|KB|MB|GB|TB|KiB|MiB|GiB|TiB)\b").unwrap(), "<SIZE>"),
        // Durations (123ms, 45.6s, 1.2min)
        (Regex::new(r"\b\d+(\.\d+)?\s*(ns|us|ms|s|sec|min|h|hr|hours?|minutes?|seconds?)\b").unwrap(), "<DUR>"),
        // Port numbers (port 8080, :8080)
        (Regex::new(r"[:\s]port\s*\d+").unwrap(), " port <PORT>"),
        (Regex::new(r":\d{2,5}\b").unwrap(), ":<PORT>"),
        // PIDs and process IDs
        (Regex::new(r"\[(\d+)\]").unwrap(), "[<PID>]"),
        (Regex::new(r"\bpid[=:\s]*\d+").unwrap(), "pid=<PID>"),
        // Session/Request/Transaction IDs
        (Regex::new(r"\b(session|request|req|tx|transaction|conn|connection)[_\-]?id[=:\s]*\S+").unwrap(), "<SESSID>"),
        // Retry/attempt counters (1/3, 2/5, etc.)
        (Regex::new(r"\(\d+/\d+\)").unwrap(), "(<N>/<N>)"),
        (Regex::new(r"\b\d+/\d+\b").unwrap(), "<N>/<N>"),
        // Percentage values
        (Regex::new(r"\b\d+(\.\d+)?%").unwrap(), "<PCT>"),
        // Generic numbers (do this last to catch remaining numbers)
        // But preserve small numbers that might be meaningful (like error codes)
        (Regex::new(r"\b\d{5,}\b").unwrap(), "<N>"),  // 5+ digit numbers
        (Regex::new(r"\b\d+\.\d+\.\d+").unwrap(), "<VER>"),  // Version numbers like 1.2.3
    ]
});

pub struct NormalizeRegex;

impl NormalizeRegex {
    pub fn new() -> Self {
        Self
    }

    /// Aggressively normalize message for grouping similar messages together
    pub fn normalize(&self, msg: &str) -> String {
        let mut result = msg.to_string();

        for (pattern, replacement) in NORMALIZE_PATTERNS.iter() {
            result = pattern.replace_all(&result, *replacement).to_string();
        }

        // Collapse multiple spaces
        static SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
        result = SPACE_RE.replace_all(&result, " ").to_string();

        result.trim().to_string()
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
