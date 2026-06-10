//! Shared log-line parser used by every log source (file import, SSH
//! streaming, ...). `parse_log_line` is the single entry point: it tries each
//! known format in priority order and, as a last resort, keeps the raw line so
//! content is never silently dropped. New formats should be added here so all
//! sources benefit at once.

use regex::Regex;
use once_cell::sync::Lazy;
use serde::Deserialize;
use crate::analyzer::LogEntry;
use crate::journalctl::JournalEntry;

/// Matches the JSON format written by `log_writer::save_logs()`.
#[derive(Deserialize)]
struct SavedJsonEntry {
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    priority: u8,
    #[serde(default)]
    service: String,
    #[serde(default)]
    message: String,
}

/// Matches the plaintext format written by `log_writer::save_logs()`:
/// "2026-02-11 10:30:45 sshd[6]: message"
/// "2026-02-11 10:30:45.123456 sshd[6]: message"
static SAVED_PLAINTEXT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}(?:\.\d+)?)\s+(\S+)\[(\d+)\]:\s*(.*)$").unwrap()
});

/// Matches an ISO-8601 / RFC-3339 timestamp at the start of a line, e.g.
/// "2024-01-15T10:30:45.123Z rest..." or "2024-01-15 10:30:45+02:00 rest...".
/// Accepts a 'T' or space date/time separator, optional fractional seconds
/// (with '.' or ','), and an optional 'Z' or numeric timezone offset.
static ISO_TIMESTAMP_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\s+(.*)$").unwrap()
});

/// Optional "host service[pid]:" prefix following a timestamp, as emitted by
/// `journalctl -o short-iso`. Used to peel a service name off the remainder.
static SYSLOG_TAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\S+)\s+([^\[:\s]+)(?:\[(\d+)\])?:\s*(.*)$").unwrap()
});

/// Parse a single log line into a [`LogEntry`], trying each known format in
/// priority order:
///   1. journalctl (BSD syslog text, or `-o json`)
///   2. jlog's own saved formats (JSON / plaintext)
///   3. ISO-8601 / RFC-3339 timestamped lines
///   4. raw fallback (keep the whole line as the message)
///
/// Returns `None` only for blank lines.
pub fn parse_log_line(line: &str, line_num: usize) -> Option<LogEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if let Some(entry) = parse_journal_line(line) {
        return Some(journal_to_log_entry(line_num, &entry));
    }
    if let Some(entry) = parse_saved_line(line, line_num) {
        return Some(entry);
    }
    if let Some(entry) = parse_iso_line(line, line_num) {
        return Some(entry);
    }
    parse_raw_line(line, line_num)
}

/// journalctl output: BSD syslog text, or a `-o json` object. The JSON branch
/// is guarded so that arbitrary JSON (jlog's own saved format, or an app's
/// structured logs) doesn't deserialize into an all-empty `JournalEntry` and
/// swallow the line — we require at least a journal message or timestamp.
fn parse_journal_line(line: &str) -> Option<JournalEntry> {
    if let Some(entry) = JournalEntry::from_syslog_line(line) {
        return Some(entry);
    }

    if line.starts_with('{') {
        if let Ok(entry) = serde_json::from_str::<JournalEntry>(line) {
            if entry.message.is_some() || entry.realtime_timestamp.is_some() {
                return Some(entry);
            }
        }
    }

    None
}

/// Parse lines in the format saved by `log_writer` (both JSON and plaintext).
fn parse_saved_line(line: &str, line_num: usize) -> Option<LogEntry> {
    // Saved JSON: {"line":1,"timestamp":"...","priority":6,"service":"sshd","message":"..."}
    // Guarded like parse_journal_line: a JSON object with neither a timestamp
    // nor a message isn't really our saved format — let it fall through to the
    // raw fallback so the original text is preserved.
    if line.starts_with('{') {
        if let Ok(saved) = serde_json::from_str::<SavedJsonEntry>(line) {
            if !saved.timestamp.is_empty() || !saved.message.is_empty() {
                return Some(LogEntry {
                    line_num,
                    timestamp: saved.timestamp,
                    priority: saved.priority,
                    service: saved.service,
                    message: saved.message,
                });
            }
        }
    }

    // Saved plaintext: "2026-02-11 10:30:45 sshd[6]: message"
    if let Some(caps) = SAVED_PLAINTEXT_REGEX.captures(line) {
        return Some(LogEntry {
            line_num,
            timestamp: caps[1].to_string(),
            service: caps[2].to_string(),
            priority: caps[3].parse().unwrap_or(6),
            message: caps[4].to_string(),
        });
    }

    None
}

/// Parse a line beginning with an ISO-8601 / RFC-3339 timestamp (the format
/// used by `journalctl -o short-iso`, Docker, Kubernetes, and most app logs).
/// The 'T' date/time separator is normalized to a space for display; the
/// fractional seconds and timezone offset are preserved as written.
fn parse_iso_line(line: &str, line_num: usize) -> Option<LogEntry> {
    let caps = ISO_TIMESTAMP_REGEX.captures(line)?;

    let timestamp = caps[1].replacen('T', " ", 1);
    let rest = caps.get(2).map_or("", |m| m.as_str());

    // Best-effort: peel a "host service[pid]:" prefix if present, otherwise the
    // remainder is treated as a plain message with no service.
    let (service, message) = match SYSLOG_TAIL_REGEX.captures(rest) {
        Some(tail) => (tail[2].to_string(), tail[4].to_string()),
        None => (String::new(), rest.to_string()),
    };

    let priority = crate::journalctl::infer_priority(&message);
    Some(LogEntry { line_num, timestamp, priority, service, message })
}

/// Last-resort fallback: keep an unrecognized line verbatim as the message,
/// with no timestamp/service and a priority inferred from its content. Returns
/// `None` only for blank lines.
fn parse_raw_line(line: &str, line_num: usize) -> Option<LogEntry> {
    if line.is_empty() {
        return None;
    }
    Some(LogEntry {
        line_num,
        timestamp: String::new(),
        priority: crate::journalctl::infer_priority(line),
        service: String::new(),
        message: line.to_string(),
    })
}

/// Convert a parsed [`JournalEntry`] into a display-ready [`LogEntry`],
/// formatting the realtime timestamp (microseconds since the epoch) into a
/// human-readable string.
fn journal_to_log_entry(line_num: usize, entry: &JournalEntry) -> LogEntry {
    let timestamp = entry.timestamp_micros()
        .and_then(|us| {
            let secs = us / 1_000_000;
            let nsecs = ((us % 1_000_000) * 1_000) as u32;
            chrono::DateTime::from_timestamp(secs, nsecs).map(|dt| {
                if nsecs == 0 {
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
                }
            })
        })
        .unwrap_or_default();

    LogEntry {
        line_num,
        timestamp,
        priority: entry.priority_num(),
        service: entry.service(),
        message: entry.msg().to_string(),
    }
}

#[cfg(test)]
#[path = "../tests/parser_tests.rs"]
mod tests;
