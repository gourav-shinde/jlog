use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};

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

    /// Get priority name
    pub fn priority_name(&self) -> &'static str {
        match self.priority_num() {
            0 => "EMERG",
            1 => "ALERT",
            2 => "CRIT",
            3 => "ERR",
            4 => "WARNING",
            5 => "NOTICE",
            6 => "INFO",
            7 => "DEBUG",
            _ => "UNKNOWN",
        }
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
}

/// Parse a journalctl JSON file (one JSON object per line)
pub fn parse_journal_file(path: &str) -> anyhow::Result<Vec<JournalEntry>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<JournalEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!("Warning: Failed to parse line: {}", e);
            }
        }
    }

    Ok(entries)
}
