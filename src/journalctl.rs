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

    pub fn timestamp_micros(&self) -> Option<i64> {
        self.realtime_timestamp
            .as_ref()
            .and_then(|ts| ts.parse::<i64>().ok())
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

    // Treat as UTC so the display (also UTC) preserves the exact time written in the log.
    // Syslog lines carry no per-line timezone, so converting via local timezone would shift
    // the displayed time by the viewer's UTC offset.
    chrono::NaiveDateTime::parse_from_str(&full_ts, "%Y %b %d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(timestamp: Option<&str>, priority: Option<&str>, syslog_id: Option<&str>, unit: Option<&str>, message: Option<&str>) -> JournalEntry {
        JournalEntry {
            realtime_timestamp: timestamp.map(|s| s.to_string()),
            priority: priority.map(|s| s.to_string()),
            syslog_identifier: syslog_id.map(|s| s.to_string()),
            systemd_unit: unit.map(|s| s.to_string()),
            message: message.map(|s| s.to_string()),
        }
    }

    // --- priority_num ---

    #[test]
    fn priority_num_valid() {
        assert_eq!(entry(None, Some("3"), None, None, None).priority_num(), 3);
        assert_eq!(entry(None, Some("0"), None, None, None).priority_num(), 0);
        assert_eq!(entry(None, Some("7"), None, None, None).priority_num(), 7);
    }

    #[test]
    fn priority_num_missing_defaults_to_6() {
        assert_eq!(entry(None, None, None, None, None).priority_num(), 6);
    }

    #[test]
    fn priority_num_invalid_string_defaults_to_6() {
        assert_eq!(entry(None, Some("bad"), None, None, None).priority_num(), 6);
    }

    // --- service ---

    #[test]
    fn service_prefers_syslog_identifier() {
        let e = entry(None, None, Some("sshd"), Some("ssh.service"), None);
        assert_eq!(e.service(), "sshd");
    }

    #[test]
    fn service_falls_back_to_systemd_unit() {
        let e = entry(None, None, None, Some("systemd-journald.service"), None);
        assert_eq!(e.service(), "systemd-journald.service");
    }

    #[test]
    fn service_defaults_to_unknown() {
        let e = entry(None, None, None, None, None);
        assert_eq!(e.service(), "unknown");
    }

    // --- msg ---

    #[test]
    fn msg_returns_message() {
        let e = entry(None, None, None, None, Some("hello world"));
        assert_eq!(e.msg(), "hello world");
    }

    #[test]
    fn msg_empty_when_missing() {
        let e = entry(None, None, None, None, None);
        assert_eq!(e.msg(), "");
    }

    // --- timestamp_micros ---

    #[test]
    fn timestamp_micros_parses_valid() {
        let e = entry(Some("1700000000000000"), None, None, None, None);
        assert_eq!(e.timestamp_micros(), Some(1_700_000_000_000_000i64));
    }

    #[test]
    fn timestamp_micros_none_when_missing() {
        let e = entry(None, None, None, None, None);
        assert!(e.timestamp_micros().is_none());
    }

    #[test]
    fn timestamp_micros_none_when_non_numeric() {
        let e = entry(Some("not-a-number"), None, None, None, None);
        assert!(e.timestamp_micros().is_none());
    }

    // --- infer_priority ---

    #[test]
    fn infer_priority_critical_keywords() {
        assert_eq!(infer_priority("kernel panic occurred"), 2);
        assert_eq!(infer_priority("Fatal error"), 2);
        assert_eq!(infer_priority("CRITICAL failure"), 2);
    }

    #[test]
    fn infer_priority_error_keywords() {
        assert_eq!(infer_priority("connection failed"), 3);
        assert_eq!(infer_priority("Error opening file"), 3);
        assert_eq!(infer_priority("operation failure"), 3);
        assert_eq!(infer_priority("cannot connect"), 3);
        assert_eq!(infer_priority("unable to bind"), 3);
        assert_eq!(infer_priority("segfault at 0x0"), 3);
        assert_eq!(infer_priority("NullPointerException"), 3);
    }

    #[test]
    fn infer_priority_warning_keywords() {
        assert_eq!(infer_priority("this is a warning"), 4);
        assert_eq!(infer_priority("warn: low disk"), 4);
        assert_eq!(infer_priority("connection timeout"), 4);
        assert_eq!(infer_priority("request timed out"), 4);
        assert_eq!(infer_priority("retrying connection"), 4);
        assert_eq!(infer_priority("deprecated API used"), 4);
        assert_eq!(infer_priority("access denied"), 4);
        assert_eq!(infer_priority("connection refused"), 4);
    }

    #[test]
    fn infer_priority_notice_keywords() {
        assert_eq!(infer_priority("service started"), 5);
        assert_eq!(infer_priority("server stopped"), 5);
        assert_eq!(infer_priority("client connected"), 5);
        assert_eq!(infer_priority("peer disconnected"), 5);
        assert_eq!(infer_priority("config loaded"), 5);
        assert_eq!(infer_priority("task finished"), 5);
    }

    #[test]
    fn infer_priority_default_info() {
        assert_eq!(infer_priority("hello world"), 6);
        assert_eq!(infer_priority(""), 6);
        assert_eq!(infer_priority("normal log line"), 6);
    }

    // --- from_syslog_line ---

    #[test]
    fn from_syslog_line_valid() {
        let line = "May 29 10:30:45 myhostname sshd[1234]: Accepted publickey";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.service(), "sshd");
        assert_eq!(e.msg(), "Accepted publickey");
        assert!(e.timestamp_micros().is_some());
    }

    #[test]
    fn from_syslog_line_without_pid() {
        let line = "Jan  1 00:00:01 host kernel: some kernel message";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.service(), "kernel");
        assert_eq!(e.msg(), "some kernel message");
    }

    #[test]
    fn from_syslog_line_garbage_returns_none() {
        assert!(JournalEntry::from_syslog_line("").is_none());
        assert!(JournalEntry::from_syslog_line("not a log line").is_none());
        assert!(JournalEntry::from_syslog_line("{\"json\": true}").is_none());
    }

    #[test]
    fn from_syslog_line_infers_priority_from_message() {
        let line = "May 29 10:30:45 host sshd[1]: connection failed";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.priority_num(), 3); // error keyword
    }

    // --- JSON deserialization ---

    #[test]
    fn deserialize_full_json_entry() {
        let json = r#"{
            "__REALTIME_TIMESTAMP": "1700000000000000",
            "PRIORITY": "4",
            "SYSLOG_IDENTIFIER": "myapp",
            "_SYSTEMD_UNIT": "myapp.service",
            "MESSAGE": "hello"
        }"#;
        let e: JournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.priority_num(), 4);
        assert_eq!(e.service(), "myapp");
        assert_eq!(e.msg(), "hello");
        assert_eq!(e.timestamp_micros(), Some(1_700_000_000_000_000i64));
    }

    #[test]
    fn deserialize_minimal_json_entry() {
        let json = r#"{"MESSAGE": "bare minimum"}"#;
        let e: JournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.msg(), "bare minimum");
        assert_eq!(e.priority_num(), 6);
        assert_eq!(e.service(), "unknown");
        assert!(e.timestamp_micros().is_none());
    }
}
