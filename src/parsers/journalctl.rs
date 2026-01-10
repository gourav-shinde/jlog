use serde::Deserialize;
use crate::helper::BufferedFileReader;

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
/// Uses BufferedFileReader for efficient handling of large files
pub fn parse_journal_file(path: &str) -> anyhow::Result<Vec<JournalEntry>> {
    let reader = BufferedFileReader::with_buffer_size(path, 64 * 1024); // 64KB buffer for large files
    let mut entries = Vec::new();
    let mut parse_errors = 0;

    let file_size = reader.file_size().unwrap_or(0);
    let use_progress = file_size > 10 * 1024 * 1024; // Show progress for files > 10MB

    if use_progress {
        reader.read_lines_with_progress(
            |_, line| {
                if !line.trim().is_empty() {
                    match serde_json::from_str::<JournalEntry>(line) {
                        Ok(entry) => entries.push(entry),
                        Err(_) => parse_errors += 1,
                    }
                }
                Ok(())
            },
            |lines, percent| {
                eprint!("\rParsing: {} lines ({:.1}%)...", lines, percent);
            },
            10000, // Update progress every 10k lines
        )?;
        eprintln!("\rParsing complete: {} entries loaded.      ", entries.len());
    } else {
        reader.read_lines(|_, line| {
            if !line.trim().is_empty() {
                match serde_json::from_str::<JournalEntry>(line) {
                    Ok(entry) => entries.push(entry),
                    Err(_) => parse_errors += 1,
                }
            }
            Ok(())
        })?;
    }

    if parse_errors > 0 {
        eprintln!("Warning: {} lines failed to parse", parse_errors);
    }

    Ok(entries)
}
