use serde::Deserialize;
use regex::Regex;
use once_cell::sync::Lazy;

/// Regex for parsing syslog format: "Mon DD HH:MM:SS[.microsecs] hostname service[pid]: message"
/// Supports both standard syslog and journalctl short-precise format with microseconds
static SYSLOG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([A-Za-z]{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})(?:\.\d+)?\s+(\S+)\s+([^\[:]+)(?:\[(\d+)\])?:\s*(.*)$").unwrap()
});

/// Represents a single journal entry from journalctl JSON output
#[derive(Debug, Deserialize, Clone)]
pub struct JournalEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    pub realtime_timestamp: Option<String>,

    #[serde(rename = "_HOSTNAME")]
    pub hostname: Option<String>,

    #[serde(rename = "PRIORITY")]
    pub priority: Option<String>,

    #[serde(rename = "SYSLOG_IDENTIFIER")]
    pub syslog_identifier: Option<String>,

    #[serde(rename = "_PID")]
    pub pid: Option<String>,

    #[serde(rename = "_SYSTEMD_UNIT")]
    pub systemd_unit: Option<String>,

    #[serde(rename = "MESSAGE")]
    pub message: Option<String>,

    #[serde(rename = "_TRANSPORT")]
    pub transport: Option<String>,
}

impl JournalEntry {
    /// Get priority as a numeric value (0-7)
    pub fn priority_num(&self) -> u8 {
        self.priority
            .as_ref()
            .and_then(|p| p.parse().ok())
            .unwrap_or(6) // default to info
    }

    /// Get the service/identifier name
    pub fn service(&self) -> String {
        self.syslog_identifier
            .clone()
            .or_else(|| self.systemd_unit.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Get the message content
    pub fn msg(&self) -> &str {
        self.message.as_deref().unwrap_or("")
    }

    /// Get timestamp as Unix seconds (journalctl uses microseconds)
    pub fn timestamp_secs(&self) -> Option<i64> {
        self.realtime_timestamp
            .as_ref()
            .and_then(|ts| ts.parse::<i64>().ok())
            .map(|us| us / 1_000_000)
    }

    /// Get hour bucket (YYYY-MM-DD HH:00) for time-series grouping
    pub fn hour_bucket(&self) -> Option<String> {
        self.timestamp_secs().map(|secs| {
            let hour = (secs / 3600) * 3600;
            chrono::DateTime::from_timestamp(hour, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:00").to_string())
                .unwrap_or_else(|| format!("{}", hour))
        })
    }

    /// Get minute bucket (YYYY-MM-DD HH:MM) for fine-grained time-series grouping
    pub fn minute_bucket(&self) -> Option<String> {
        self.timestamp_secs().map(|secs| {
            let minute = (secs / 60) * 60;
            chrono::DateTime::from_timestamp(minute, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| format!("{}", minute))
        })
    }

    /// Parse a syslog/text format line into a JournalEntry
    /// Format: "Mon DD HH:MM:SS hostname service[pid]: message"
    pub fn from_syslog_line(line: &str) -> Option<Self> {
        let caps = SYSLOG_REGEX.captures(line)?;

        let timestamp_str = caps.get(1)?.as_str();
        let hostname = caps.get(2).map(|m| m.as_str().to_string());
        let service = caps.get(3).map(|m| m.as_str().trim().to_string());
        let pid = caps.get(4).map(|m| m.as_str().to_string());
        let message = caps.get(5).map(|m| m.as_str().to_string());

        // Parse timestamp - assume current year since syslog doesn't include it
        let timestamp = parse_syslog_timestamp(timestamp_str);

        // Infer priority from message content
        let priority = infer_priority(message.as_deref().unwrap_or(""));

        Some(JournalEntry {
            realtime_timestamp: timestamp.map(|t| (t * 1_000_000).to_string()),
            hostname,
            priority: Some(priority.to_string()),
            syslog_identifier: service,
            pid,
            systemd_unit: None,
            message,
            transport: None,
        })
    }
}

/// Parse syslog timestamp (e.g., "Jan 10 16:42:10") to Unix seconds
fn parse_syslog_timestamp(ts: &str) -> Option<i64> {
    let now = chrono::Local::now();
    let year = now.format("%Y").to_string();
    let full_ts = format!("{} {}", year, ts);

    chrono::NaiveDateTime::parse_from_str(&full_ts, "%Y %b %d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_local_timezone(chrono::Local).single())
        .flatten()
        .map(|dt| dt.timestamp())
}

/// Infer syslog priority from message content
/// Returns: 0=emerg, 1=alert, 2=crit, 3=err, 4=warning, 5=notice, 6=info, 7=debug
fn infer_priority(msg: &str) -> u8 {
    let msg_lower = msg.to_lowercase();

    // Critical patterns (priority 2)
    if msg_lower.contains("panic") || msg_lower.contains("fatal") || msg_lower.contains("critical") {
        return 2;
    }

    // Error patterns (priority 3)
    if msg_lower.contains("error") || msg_lower.contains("failed") || msg_lower.contains("failure")
        || msg_lower.contains("cannot") || msg_lower.contains("unable to")
        || msg_lower.contains("segfault") || msg_lower.contains("exception")
    {
        return 3;
    }

    // Warning patterns (priority 4)
    if msg_lower.contains("warning") || msg_lower.contains("warn")
        || msg_lower.contains("timeout") || msg_lower.contains("timed out")
        || msg_lower.contains("retrying") || msg_lower.contains("deprecated")
        || msg_lower.contains("denied") || msg_lower.contains("refused")
    {
        return 4;
    }

    // Notice patterns (priority 5)
    if msg_lower.contains("started") || msg_lower.contains("stopped")
        || msg_lower.contains("connected") || msg_lower.contains("disconnected")
        || msg_lower.contains("loaded") || msg_lower.contains("finished")
    {
        return 5;
    }

    // Default to info (priority 6)
    6
}
