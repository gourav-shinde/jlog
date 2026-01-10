use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::time::Duration;
use std::thread;
use colored::Colorize;
use regex::Regex;

use crate::journalctl::JournalEntry;

/// Filter criteria for monitor mode
pub struct MonitorFilter {
    pub unit: Option<String>,
    pub max_priority: u8,
    pub pattern: Option<Regex>,
}

impl MonitorFilter {
    pub fn new(unit: Option<String>, priority: u8, pattern: Option<String>) -> anyhow::Result<Self> {
        let pattern = pattern.map(|p| Regex::new(&p)).transpose()?;
        Ok(Self {
            unit,
            max_priority: priority,
            pattern,
        })
    }

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

/// Live statistics tracker
struct LiveStats {
    total: usize,
    by_priority: [usize; 8],
    alerts: Vec<String>,
}

impl LiveStats {
    fn new() -> Self {
        Self {
            total: 0,
            by_priority: [0; 8],
            alerts: Vec::new(),
        }
    }

    fn record(&mut self, entry: &JournalEntry) {
        self.total += 1;
        let priority = entry.priority_num() as usize;
        if priority < 8 {
            self.by_priority[priority] += 1;
        }

        // Check for alert patterns
        let msg = entry.msg();
        if msg.contains("Failed password") {
            self.alerts.push("SSH auth failure detected".to_string());
        }
        if msg.contains("Out of memory") || msg.contains("OOM") {
            self.alerts.push("OOM event detected!".to_string());
        }
        if msg.contains("error") || msg.contains("ERROR") {
            if entry.priority_num() <= 3 {
                self.alerts.push(format!("Error from {}", entry.service()));
            }
        }
    }
}

/// Format a log entry for display
fn format_entry(entry: &JournalEntry) -> String {
    let priority = entry.priority_num();
    let priority_label = match priority {
        0 => "EMERG  ".red().bold(),
        1 => "ALERT  ".red().bold(),
        2 => "CRIT   ".red().bold(),
        3 => "ERR    ".red(),
        4 => "WARN   ".yellow(),
        5 => "NOTICE ".blue(),
        6 => "INFO   ".normal(),
        7 => "DEBUG  ".dimmed(),
        _ => "???    ".normal(),
    };

    let service = format!("{:15}", entry.service()).cyan();
    let msg = entry.msg();

    // Truncate long messages
    let msg_display = if msg.len() > 100 {
        format!("{}...", &msg[..97])
    } else {
        msg.to_string()
    };

    format!("{} {} {}", priority_label, service, msg_display)
}

/// Print status bar
fn print_status_bar(stats: &LiveStats) {
    let errors = stats.by_priority[0] + stats.by_priority[1] + stats.by_priority[2] + stats.by_priority[3];
    let warnings = stats.by_priority[4];

    eprint!("\r{} | Total: {} | Errors: {} | Warnings: {} | {}",
        "MONITORING".green().bold(),
        stats.total.to_string().cyan(),
        errors.to_string().red(),
        warnings.to_string().yellow(),
        "Ctrl+C to stop".dimmed()
    );
}

/// Main monitor function - tail a file for new entries
pub fn monitor(path: Option<String>, unit: Option<String>, priority: u8, pattern: Option<String>) -> anyhow::Result<()> {
    let filter = MonitorFilter::new(unit, priority, pattern)?;
    let mut stats = LiveStats::new();

    // Print header
    println!("{}", "─".repeat(80).dimmed());
    println!("{}", "jlog - Real-time Log Monitor".bold());
    println!("{}", "─".repeat(80).dimmed());

    match path {
        Some(file_path) => {
            println!("Watching: {}", file_path.cyan());
            println!("Filters: priority <= {}, unit: {}, pattern: {}",
                priority,
                filter.unit.as_deref().unwrap_or("any").yellow(),
                filter.pattern.as_ref().map(|p| p.as_str()).unwrap_or("none").yellow()
            );
            println!("{}", "─".repeat(80).dimmed());
            println!();

            tail_file(&file_path, &filter, &mut stats)?;
        }
        None => {
            println!("Reading from: {}", "stdin".cyan());
            println!("Pipe journalctl output: {}", "journalctl -f -o json | jlog monitor".dimmed());
            println!("{}", "─".repeat(80).dimmed());
            println!();

            read_stdin(&filter, &mut stats)?;
        }
    }

    Ok(())
}

/// Tail a file for new entries (like tail -f)
fn tail_file(path: &str, filter: &MonitorFilter, stats: &mut LiveStats) -> anyhow::Result<()> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Seek to end of file
    reader.seek(SeekFrom::End(0))?;

    let mut line = String::new();
    let mut last_status_update = std::time::Instant::now();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // No new data, wait and retry
                thread::sleep(Duration::from_millis(100));

                // Update status bar periodically
                if last_status_update.elapsed() > Duration::from_secs(1) {
                    print_status_bar(stats);
                    last_status_update = std::time::Instant::now();
                }
            }
            Ok(_) => {
                process_line(&line, filter, stats);
            }
            Err(e) => {
                eprintln!("{}", format!("Error reading file: {}", e).red());
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

/// Read from stdin (for piped input)
fn read_stdin(filter: &MonitorFilter, stats: &mut LiveStats) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let reader = stdin.lock();

    for line in reader.lines() {
        match line {
            Ok(line) => {
                process_line(&line, filter, stats);
            }
            Err(e) => {
                eprintln!("{}", format!("Error reading stdin: {}", e).red());
            }
        }
    }

    Ok(())
}

/// Process a single line
fn process_line(line: &str, filter: &MonitorFilter, stats: &mut LiveStats) {
    if line.trim().is_empty() {
        return;
    }

    match serde_json::from_str::<JournalEntry>(line) {
        Ok(entry) => {
            if filter.matches(&entry) {
                stats.record(&entry);

                // Clear status bar line and print entry
                eprint!("\r{}\r", " ".repeat(80));
                println!("{}", format_entry(&entry));

                // Print any alerts
                for alert in stats.alerts.drain(..) {
                    println!("  {} {}", "⚠".yellow(), alert.yellow());
                }
            }
        }
        Err(_) => {
            // Silently ignore non-JSON lines
        }
    }
}
