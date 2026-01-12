pub mod state;
pub mod filter;
pub mod output;

use std::cell::Cell;
use colored::Colorize;

use crate::helper::BufferedFileReader;
use crate::journalctl::JournalEntry;

pub use state::AnalysisState;
pub use filter::{FilterCriteria, NormalizeRegex};

/// Main analyze function - streaming implementation
pub fn analyze(
    path: Option<String>,
    unit: Option<String>,
    priority: u8,
    top: usize,
    pattern: Option<String>,
    report_path: Option<String>,
    serve: bool,
    port: u16,
) -> anyhow::Result<()> {
    let path = match path {
        Some(p) => p,
        None => {
            println!("{}", "Error: No path provided. Use --path <file>".red());
            return Ok(());
        }
    };

    println!("{}", format!("Analyzing: {}", path).cyan());
    println!("{}", "─".repeat(60).dimmed());

    // Set up streaming analysis
    let reader = BufferedFileReader::with_buffer_size(&path, 128 * 1024);
    let filter = FilterCriteria::new(unit, priority, pattern)?;
    let normalize_regex = NormalizeRegex::new();
    let mut state = AnalysisState::new();

    let file_size = reader.file_size().unwrap_or(0);
    let mut lines_read = 0usize;
    let mut parse_errors = 0usize;

    let show_progress = file_size > 10 * 1024 * 1024;
    let progress_interval = if file_size > 500 * 1024 * 1024 { 100_000 } else { 50_000 };

    let matched_count = Cell::new(0usize);

    if show_progress {
        reader.read_lines_with_progress(
            |_, line| {
                lines_read += 1;
                if let Some(entry) = parse_line(line, &mut parse_errors) {
                    if filter.matches(&entry) {
                        state.process_entry(&entry, &normalize_regex);
                        matched_count.set(state.total_entries);
                    }
                }
                Ok(())
            },
            |lines, percent| {
                eprint!("\rProcessing: {} lines ({:.1}%) - {} entries matched...",
                    lines, percent, matched_count.get());
            },
            progress_interval,
        )?;
        eprintln!("\r{}", " ".repeat(70));
    } else {
        reader.read_lines(|_, line| {
            lines_read += 1;
            if let Some(entry) = parse_line(line, &mut parse_errors) {
                if filter.matches(&entry) {
                    state.process_entry(&entry, &normalize_regex);
                }
            }
            Ok(())
        })?;
    }

    if parse_errors > 0 {
        eprintln!("{}", format!("Warning: {} lines failed to parse", parse_errors).yellow());
    }

    if state.total_entries == 0 {
        println!("{}", "No entries match the specified filters.".yellow());
        return Ok(());
    }

    // Generate HTML report if requested
    if let Some(ref report_file) = report_path {
        crate::report::generate_html_report(&state, report_file, lines_read)?;
        println!("{}", format!("Report saved to: {}", report_file).green());
    }

    // Start live server if requested
    if serve {
        crate::server::start_server(&state, port, lines_read)?;
    } else {
        // Display terminal results
        output::print_summary(&state, lines_read);
        output::print_priority_breakdown(&state);
        output::print_time_series(&state);
        output::print_service_breakdown(&state, top);
        output::print_top_errors(&state, top);
        output::print_patterns(&state);
    }

    Ok(())
}

fn parse_line(line: &str, parse_errors: &mut usize) -> Option<JournalEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Try text/syslog format first (most common for log files)
    if let Some(entry) = JournalEntry::from_syslog_line(line) {
        return Some(entry);
    }

    // Fall back to JSON format (journalctl -o json)
    if line.starts_with('{') {
        if let Ok(entry) = serde_json::from_str::<JournalEntry>(line) {
            return Some(entry);
        }
    }

    *parse_errors += 1;
    None
}
