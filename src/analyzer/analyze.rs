use std::collections::HashMap;
use std::cell::Cell;
use colored::Colorize;
use regex::Regex;

use crate::helper::BufferedFileReader;
use crate::journalctl::JournalEntry;

/// Streaming analysis state - accumulates statistics without storing entries
pub struct AnalysisState {
    pub total_entries: usize,
    pub entries_by_priority: [usize; 8],
    pub entries_by_service: HashMap<String, usize>,
    pub error_messages: HashMap<String, usize>,
    pub max_errors_tracked: usize,

    // Pattern counters
    pub failed_ssh_count: usize,
    pub restart_count: usize,
    pub oom_count: usize,
    pub timeout_count: usize,
    pub disk_issue_count: usize,
    pub firewall_block_count: usize,
}

impl AnalysisState {
    pub fn new(max_errors: usize) -> Self {
        Self {
            total_entries: 0,
            entries_by_priority: [0; 8],
            entries_by_service: HashMap::new(),
            error_messages: HashMap::new(),
            max_errors_tracked: max_errors,
            failed_ssh_count: 0,
            restart_count: 0,
            oom_count: 0,
            timeout_count: 0,
            disk_issue_count: 0,
            firewall_block_count: 0,
        }
    }

    /// Process a single entry - called for each line during streaming
    pub fn process_entry(&mut self, entry: &JournalEntry, normalize_regex: &NormalizeRegex) {
        self.total_entries += 1;

        // Count by priority
        let priority = entry.priority_num() as usize;
        if priority < 8 {
            self.entries_by_priority[priority] += 1;
        }

        // Count by service
        *self.entries_by_service.entry(entry.service()).or_insert(0) += 1;

        // Track error messages (priority <= 4: warning and above)
        if entry.priority_num() <= 4 {
            let msg = normalize_message(entry.msg(), normalize_regex);
            if !msg.is_empty() {
                *self.error_messages.entry(msg).or_insert(0) += 1;
            }
        }

        // Detect patterns inline
        self.detect_patterns(entry);
    }

    /// Detect patterns for a single entry
    fn detect_patterns(&mut self, entry: &JournalEntry) {
        let msg = entry.msg();
        let msg_lower = msg.to_lowercase();

        // Failed SSH
        if msg.contains("Failed password") {
            self.failed_ssh_count += 1;
        }

        // Service restarts
        if msg.contains("restart") || msg.contains("Restarting") {
            self.restart_count += 1;
        }

        // OOM
        if msg.contains("Out of memory") || msg.contains("OOM") {
            self.oom_count += 1;
        }

        // Timeouts
        if msg_lower.contains("timeout") {
            self.timeout_count += 1;
        }

        // Disk issues
        if msg_lower.contains("disk") &&
           (msg_lower.contains("error") || msg_lower.contains("full") || msg_lower.contains("90%")) {
            self.disk_issue_count += 1;
        }

        // Firewall blocks
        if msg.contains("UFW BLOCK") || msg.contains("BLOCKED") {
            self.firewall_block_count += 1;
        }
    }

    /// Get top N error messages sorted by count
    pub fn top_errors(&self, n: usize) -> Vec<(String, usize)> {
        let mut errors: Vec<(String, usize)> = self.error_messages.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        errors.sort_by(|a, b| b.1.cmp(&a.1));
        errors.truncate(n);
        errors
    }

    /// Get top N services sorted by count
    pub fn top_services(&self, n: usize) -> Vec<(String, usize)> {
        let mut services: Vec<(String, usize)> = self.entries_by_service.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        services.sort_by(|a, b| b.1.cmp(&a.1));
        services.truncate(n);
        services
    }
}

/// Pre-compiled regex patterns for message normalization
pub struct NormalizeRegex {
    ip: Regex,
    port: Regex,
    pid: Regex,
    container_id: Regex,
}

impl NormalizeRegex {
    pub fn new() -> Self {
        Self {
            ip: Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap(),
            port: Regex::new(r"port \d+").unwrap(),
            pid: Regex::new(r"\[\d+\]").unwrap(),
            container_id: Regex::new(r"[a-f0-9]{12,}").unwrap(),
        }
    }
}

/// Normalize message for grouping (remove variable parts)
fn normalize_message(msg: &str, regex: &NormalizeRegex) -> String {
    let msg = regex.ip.replace_all(msg, "<IP>");
    let msg = regex.port.replace_all(&msg, "port <PORT>");
    let msg = regex.pid.replace_all(&msg, "[<PID>]");
    let msg = regex.container_id.replace_all(&msg, "<ID>");
    msg.to_string()
}

/// Filter criteria for entries
pub struct FilterCriteria {
    pub unit: Option<String>,
    pub max_priority: u8,
    pub pattern: Option<Regex>,
}

impl FilterCriteria {
    pub fn new(unit: Option<String>, max_priority: u8, pattern: Option<String>) -> anyhow::Result<Self> {
        let pattern = pattern.map(|p| Regex::new(&p)).transpose()?;
        Ok(Self { unit, max_priority, pattern })
    }

    /// Check if entry passes all filters
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

/// Main analyze function - streaming implementation
pub fn analyze(path: Option<String>, unit: Option<String>, priority: u8, top: usize, pattern: Option<String>) -> anyhow::Result<()> {
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
    let reader = BufferedFileReader::with_buffer_size(&path, 128 * 1024); // 128KB buffer
    let filter = FilterCriteria::new(unit, priority, pattern)?;
    let normalize_regex = NormalizeRegex::new();
    let mut state = AnalysisState::new(top);

    let file_size = reader.file_size().unwrap_or(0);
    let mut lines_read = 0usize;
    let mut parse_errors = 0usize;

    // Determine if we should show progress (files > 10MB)
    let show_progress = file_size > 10 * 1024 * 1024;
    let progress_interval = if file_size > 500 * 1024 * 1024 { 100_000 } else { 50_000 };

    // Use Cell for progress counter that can be read from progress callback
    let matched_count = Cell::new(0usize);

    // Stream and process
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
        eprintln!("\r{}", " ".repeat(70)); // Clear progress line
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

    // Report parsing issues
    if parse_errors > 0 {
        eprintln!("{}", format!("Warning: {} lines failed to parse", parse_errors).yellow());
    }

    if state.total_entries == 0 {
        println!("{}", "No entries match the specified filters.".yellow());
        return Ok(());
    }

    // Display results
    print_summary(&state, lines_read);
    print_priority_breakdown(&state);
    print_service_breakdown(&state, top);
    print_top_errors(&state, top);
    print_patterns(&state);

    Ok(())
}

/// Parse a single JSON line
fn parse_line(line: &str, parse_errors: &mut usize) -> Option<JournalEntry> {
    if line.trim().is_empty() {
        return None;
    }

    match serde_json::from_str::<JournalEntry>(line) {
        Ok(entry) => Some(entry),
        Err(_) => {
            *parse_errors += 1;
            None
        }
    }
}

// ============ Output Functions ============

fn print_summary(state: &AnalysisState, lines_read: usize) {
    println!("\n{}", "SUMMARY".bold().underline());
    println!("  Lines read:             {}", lines_read.to_string().dimmed());
    println!("  Entries matched:        {}", state.total_entries.to_string().cyan());

    let critical = state.entries_by_priority[0] + state.entries_by_priority[1] + state.entries_by_priority[2];
    let errors = state.entries_by_priority[3];
    let warnings = state.entries_by_priority[4];

    println!("  Critical/Alert/Emerg:   {}", critical.to_string().red().bold());
    println!("  Errors:                 {}", errors.to_string().red());
    println!("  Warnings:               {}", warnings.to_string().yellow());
}

fn print_priority_breakdown(state: &AnalysisState) {
    println!("\n{}", "PRIORITY DISTRIBUTION".bold().underline());

    let priority_names = [
        "EMERG  ", "ALERT  ", "CRIT   ", "ERR    ",
        "WARNING", "NOTICE ", "INFO   ", "DEBUG  ",
    ];

    let max_count = state.entries_by_priority.iter().max().copied().unwrap_or(1);

    for (priority, name) in priority_names.iter().enumerate() {
        let count = state.entries_by_priority[priority];
        if count == 0 {
            continue;
        }

        let bar_width = ((count as f64 / max_count as f64) * 30.0) as usize;
        let bar = "█".repeat(bar_width.max(1));

        let colored_name = match priority {
            0..=2 => name.red().bold(),
            3 => name.red(),
            4 => name.yellow(),
            5 => name.blue(),
            _ => name.normal(),
        };

        let colored_bar = match priority {
            0..=2 => bar.red().bold(),
            3 => bar.red(),
            4 => bar.yellow(),
            5 => bar.blue(),
            _ => bar.normal(),
        };

        println!("  {} {} {}", colored_name, colored_bar, count);
    }
}

fn print_service_breakdown(state: &AnalysisState, top: usize) {
    println!("\n{}", "TOP SERVICES".bold().underline());

    let services = state.top_services(top);
    if services.is_empty() {
        println!("  {}", "No services found.".dimmed());
        return;
    }

    let max_count = services.first().map(|(_, c)| *c).unwrap_or(1);

    for (service, count) in services {
        let bar_width = ((count as f64 / max_count as f64) * 30.0) as usize;
        let bar = "█".repeat(bar_width.max(1));
        println!("  {:15} {} {}", service.cyan(), bar.dimmed(), count);
    }
}

fn print_top_errors(state: &AnalysisState, top: usize) {
    let errors = state.top_errors(top);
    if errors.is_empty() {
        return;
    }

    println!("\n{}", "TOP ERROR MESSAGES".bold().underline());

    for (i, (msg, count)) in errors.iter().enumerate() {
        let truncated = if msg.len() > 70 {
            format!("{}...", &msg[..67])
        } else {
            msg.clone()
        };

        println!("  {}. [{}x] {}",
            (i + 1).to_string().dimmed(),
            count.to_string().red(),
            truncated
        );
    }
}

/// Pattern info for display
struct PatternInfo {
    name: &'static str,
    description: String,
    severity: u8, // 0=critical, 1=warning, 2=info
}

fn print_patterns(state: &AnalysisState) {
    let mut patterns: Vec<PatternInfo> = Vec::new();

    // Collect detected patterns
    if state.failed_ssh_count >= 3 {
        let severity = if state.failed_ssh_count >= 10 { 0 } else { 1 };
        patterns.push(PatternInfo {
            name: "SSH Brute Force Attempt",
            description: format!("{} failed password attempts", state.failed_ssh_count),
            severity,
        });
    }

    if state.oom_count > 0 {
        patterns.push(PatternInfo {
            name: "Out of Memory",
            description: format!("{} OOM killer events", state.oom_count),
            severity: 0,
        });
    }

    if state.restart_count >= 2 {
        patterns.push(PatternInfo {
            name: "Service Restarts",
            description: format!("{} restart events", state.restart_count),
            severity: 1,
        });
    }

    if state.timeout_count >= 2 {
        patterns.push(PatternInfo {
            name: "Connection Timeouts",
            description: format!("{} timeout events", state.timeout_count),
            severity: 1,
        });
    }

    if state.disk_issue_count > 0 {
        patterns.push(PatternInfo {
            name: "Disk Issues",
            description: format!("{} disk-related issues", state.disk_issue_count),
            severity: 1,
        });
    }

    if state.firewall_block_count >= 2 {
        patterns.push(PatternInfo {
            name: "Firewall Blocks",
            description: format!("{} blocked connections", state.firewall_block_count),
            severity: 2,
        });
    }

    if patterns.is_empty() {
        println!("\n{}", "No concerning patterns detected.".green());
        return;
    }

    println!("\n{}", "⚠ PATTERNS DETECTED".bold().underline().yellow());

    for p in patterns {
        let (icon, colored_name) = match p.severity {
            0 => ("🔴", p.name.red().bold()),
            1 => ("🟡", p.name.yellow()),
            _ => ("🔵", p.name.blue()),
        };
        println!("  {} {}: {}", icon, colored_name, p.description);
    }
}
