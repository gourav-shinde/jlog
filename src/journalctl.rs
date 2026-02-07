use serde::Deserialize;
use regex::Regex;
use once_cell::sync::Lazy;

static SYSLOG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([A-Za-z]{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2})(?:\.\d+)?\s+(\S+)\s+([^\[:]+)(?:\[(\d+)\])?:\s*(.*)$").unwrap()
});

#[derive(Debug, Deserialize, Clone)]
pub struct JournalEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    pub realtime_timestamp: Option<String>,

    #[serde(rename = "PRIORITY")]
    pub priority: Option<String>,

    #[serde(rename = "SYSLOG_IDENTIFIER")]
    pub syslog_identifier: Option<String>,

    #[serde(rename = "_SYSTEMD_UNIT")]
    pub systemd_unit: Option<String>,

    #[serde(rename = "MESSAGE")]
    pub message: Option<String>,
}

impl JournalEntry {
    pub fn priority_num(&self) -> u8 {
        self.priority
            .as_ref()
            .and_then(|p| p.parse().ok())
            .unwrap_or(6)
    }

    pub fn service(&self) -> String {
        self.syslog_identifier
            .clone()
            .or_else(|| self.systemd_unit.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn msg(&self) -> &str {
        self.message.as_deref().unwrap_or("")
    }

    pub fn timestamp_secs(&self) -> Option<i64> {
        self.realtime_timestamp
            .as_ref()
            .and_then(|ts| ts.parse::<i64>().ok())
            .map(|us| us / 1_000_000)
    }

    pub fn from_syslog_line(line: &str) -> Option<Self> {
        let caps = SYSLOG_REGEX.captures(line)?;

        let timestamp_str = caps.get(1)?.as_str();
        let service = caps.get(3).map(|m| m.as_str().trim().to_string());
        let message = caps.get(5).map(|m| m.as_str().to_string());

        let timestamp = parse_syslog_timestamp(timestamp_str);
        let priority = infer_priority(message.as_deref().unwrap_or(""));

        Some(JournalEntry {
            realtime_timestamp: timestamp.map(|t| (t * 1_000_000).to_string()),
            priority: Some(priority.to_string()),
            syslog_identifier: service,
            systemd_unit: None,
            message,
        })
    }
}

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

fn infer_priority(msg: &str) -> u8 {
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("panic") || msg_lower.contains("fatal") || msg_lower.contains("critical") {
        return 2;
    }

    if msg_lower.contains("error") || msg_lower.contains("failed") || msg_lower.contains("failure")
        || msg_lower.contains("cannot") || msg_lower.contains("unable to")
        || msg_lower.contains("segfault") || msg_lower.contains("exception")
    {
        return 3;
    }

    if msg_lower.contains("warning") || msg_lower.contains("warn")
        || msg_lower.contains("timeout") || msg_lower.contains("timed out")
        || msg_lower.contains("retrying") || msg_lower.contains("deprecated")
        || msg_lower.contains("denied") || msg_lower.contains("refused")
    {
        return 4;
    }

    if msg_lower.contains("started") || msg_lower.contains("stopped")
        || msg_lower.contains("connected") || msg_lower.contains("disconnected")
        || msg_lower.contains("loaded") || msg_lower.contains("finished")
    {
        return 5;
    }

    6
}
