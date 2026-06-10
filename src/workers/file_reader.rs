use std::io::BufRead;
use crossbeam_channel::Sender;
use crate::background::BackgroundMessage;
use crate::workers::parser::parse_log_line;

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

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };

        bytes_processed += line.len() as u64 + 1;
        lines_read += 1;

        // The shared parser keeps unrecognized lines as raw entries, so it only
        // returns None for blank lines.
        let log_entry = match parse_log_line(&line, lines_read) {
            Some(entry) => entry,
            None => continue,
        };
        if tx.send(BackgroundMessage::Entry(log_entry)).is_err() {
            return Ok(()); // receiver dropped, stop
        }
        entries_sent += 1;

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

#[cfg(test)]
#[path = "../tests/file_reader_tests.rs"]
mod tests;
