use serde::Deserialize;

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
}
