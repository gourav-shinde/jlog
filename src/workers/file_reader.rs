use std::io::BufRead;
use crossbeam_channel::Sender;
use crate::analyzer::LogEntry;
use crate::background::BackgroundMessage;
use crate::journalctl::JournalEntry;

pub fn read_file(path: String, tx: Sender<BackgroundMessage>) {
    std::thread::spawn(move || {
        if let Err(e) = do_read(&path, &tx) {
            let _ = tx.send(BackgroundMessage::Error(format!("File read error: {}", e)));
        }
    });
}

fn do_read(path: &str, tx: &Sender<BackgroundMessage>) -> anyhow::Result<()> {
    let file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len() as f64;
    let reader = std::io::BufReader::with_capacity(128 * 1024, file);

    let mut lines_read = 0usize;
    let mut entries_sent = 0usize;
    let mut bytes_processed = 0u64;
    let mut parse_errors = 0usize;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };

        bytes_processed += line.len() as u64 + 1;
        lines_read += 1;

        if let Some(entry) = parse_line(&line, &mut parse_errors) {
            let log_entry = journal_to_log_entry(lines_read, &entry);
            if tx.send(BackgroundMessage::Entry(log_entry)).is_err() {
                return Ok(()); // receiver dropped, stop
            }
            entries_sent += 1;
        }

        if lines_read % 50_000 == 0 {
            let percent = if file_size > 0.0 {
                (bytes_processed as f32 / file_size as f32) * 100.0
            } else {
                0.0
            };
            let _ = tx.send(BackgroundMessage::Progress { lines: lines_read, percent });
        }
    }

    let _ = tx.send(BackgroundMessage::Completed {
        total_lines: lines_read,
        entries: entries_sent,
    });

    Ok(())
}

fn parse_line(line: &str, parse_errors: &mut usize) -> Option<JournalEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if let Some(entry) = JournalEntry::from_syslog_line(line) {
        return Some(entry);
    }

    if line.starts_with('{') {
        if let Ok(entry) = serde_json::from_str::<JournalEntry>(line) {
            return Some(entry);
        }
    }

    *parse_errors += 1;
    None
}

fn journal_to_log_entry(line_num: usize, entry: &JournalEntry) -> LogEntry {
    let timestamp = entry.timestamp_secs()
        .and_then(|secs| {
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
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
