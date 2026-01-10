use std::collections::HashMap;
use colored::Colorize;
use regex::Regex;

use crate::journalctl::{parse_journal_file, JournalEntry};

/// Analysis results container
pub struct AnalysisResult {
    pub total_entries: usize,
    pub entries_by_priority: HashMap<u8, usize>,
    pub entries_by_service: HashMap<String, usize>,
    pub error_messages: Vec<(String, usize)>,
    pub patterns_detected: Vec<PatternMatch>,
}

/// Detected pattern in logs
pub struct PatternMatch {
    pub name: String,
    pub description: String,
    pub count: usize,
    pub severity: Severity,
}

#[derive(Clone, Copy)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// Main analyze function
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

    // Parse the journal file
    let entries = parse_journal_file(&path)?;

    if entries.is_empty() {
        println!("{}", "No log entries found.".yellow());
        return Ok(());
    }

    // Filter entries
    let entries = filter_entries(entries, unit.as_deref(), priority, pattern.as_deref())?;

    if entries.is_empty() {
        println!("{}", "No entries match the specified filters.".yellow());
        return Ok(());
    }

    // Perform analysis
    let result = compute_statistics(&entries, top);

    // Display results
    print_summary(&result);
    print_priority_breakdown(&result);
    print_service_breakdown(&result, top);
    print_top_errors(&result);
    print_patterns(&result);

    Ok(())
}

/// Filter entries based on criteria
fn filter_entries(
    entries: Vec<JournalEntry>,
    unit: Option<&str>,
    max_priority: u8,
    pattern: Option<&str>,
) -> anyhow::Result<Vec<JournalEntry>> {
    let pattern_regex = pattern.map(|p| Regex::new(p)).transpose()?;

    let filtered: Vec<JournalEntry> = entries
        .into_iter()
        .filter(|e| {
            // Filter by priority (lower number = higher severity)
            if e.priority_num() > max_priority {
                return false;
            }

            // Filter by unit/service
            if let Some(unit_filter) = unit {
                let service = e.service().to_lowercase();
                if !service.contains(&unit_filter.to_lowercase()) {
                    return false;
                }
            }

            // Filter by pattern
            if let Some(ref regex) = pattern_regex {
                if !regex.is_match(e.msg()) {
                    return false;
                }
            }

            true
        })
        .collect();

    Ok(filtered)
}

/// Compute statistics from entries
fn compute_statistics(entries: &[JournalEntry], top: usize) -> AnalysisResult {
    let mut entries_by_priority: HashMap<u8, usize> = HashMap::new();
    let mut entries_by_service: HashMap<String, usize> = HashMap::new();
    let mut message_counts: HashMap<String, usize> = HashMap::new();

    for entry in entries {
        // Count by priority
        *entries_by_priority.entry(entry.priority_num()).or_insert(0) += 1;

        // Count by service
        *entries_by_service.entry(entry.service()).or_insert(0) += 1;

        // Count error messages (priority <= 4)
        if entry.priority_num() <= 4 {
            let msg = normalize_message(entry.msg());
            if !msg.is_empty() {
                *message_counts.entry(msg).or_insert(0) += 1;
            }
        }
    }

    // Sort error messages by count
    let mut error_messages: Vec<(String, usize)> = message_counts.into_iter().collect();
    error_messages.sort_by(|a, b| b.1.cmp(&a.1));
    error_messages.truncate(top);

    // Detect patterns
    let patterns_detected = detect_patterns(entries);

    AnalysisResult {
        total_entries: entries.len(),
        entries_by_priority,
        entries_by_service,
        error_messages,
        patterns_detected,
    }
}

/// Normalize message for grouping (remove variable parts like IPs, PIDs, etc.)
fn normalize_message(msg: &str) -> String {
    let msg = msg.to_string();

    // Remove IP addresses
    let ip_regex = Regex::new(r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}").unwrap();
    let msg = ip_regex.replace_all(&msg, "<IP>");

    // Remove port numbers
    let port_regex = Regex::new(r"port \d+").unwrap();
    let msg = port_regex.replace_all(&msg, "port <PORT>");

    // Remove PIDs
    let pid_regex = Regex::new(r"\[\d+\]").unwrap();
    let msg = pid_regex.replace_all(&msg, "[<PID>]");

    // Remove container IDs
    let container_regex = Regex::new(r"[a-f0-9]{12,}").unwrap();
    let msg = container_regex.replace_all(&msg, "<ID>");

    msg.to_string()
}

/// Detect common patterns in logs
fn detect_patterns(entries: &[JournalEntry]) -> Vec<PatternMatch> {
    let mut patterns = Vec::new();

    // Pattern: Failed SSH login attempts
    let failed_ssh: Vec<&JournalEntry> = entries
        .iter()
        .filter(|e| e.msg().contains("Failed password"))
        .collect();

    if failed_ssh.len() >= 3 {
        patterns.push(PatternMatch {
            name: "SSH Brute Force Attempt".to_string(),
            description: format!("{} failed password attempts detected", failed_ssh.len()),
            count: failed_ssh.len(),
            severity: if failed_ssh.len() >= 10 { Severity::Critical } else { Severity::Warning },
        });
    }

    // Pattern: Service restarts
    let restarts: usize = entries
        .iter()
        .filter(|e| e.msg().contains("restart") || e.msg().contains("Restarting"))
        .count();

    if restarts >= 2 {
        patterns.push(PatternMatch {
            name: "Service Restarts".to_string(),
            description: format!("{} service restart events detected", restarts),
            count: restarts,
            severity: Severity::Warning,
        });
    }

    // Pattern: Out of Memory
    let oom: usize = entries
        .iter()
        .filter(|e| e.msg().contains("Out of memory") || e.msg().contains("OOM"))
        .count();

    if oom > 0 {
        patterns.push(PatternMatch {
            name: "Out of Memory".to_string(),
            description: format!("{} OOM killer events detected", oom),
            count: oom,
            severity: Severity::Critical,
        });
    }

    // Pattern: Connection timeouts
    let timeouts: usize = entries
        .iter()
        .filter(|e| e.msg().to_lowercase().contains("timeout"))
        .count();

    if timeouts >= 2 {
        patterns.push(PatternMatch {
            name: "Connection Timeouts".to_string(),
            description: format!("{} timeout events detected", timeouts),
            count: timeouts,
            severity: Severity::Warning,
        });
    }

    // Pattern: Disk issues
    let disk_issues: usize = entries
        .iter()
        .filter(|e| {
            let msg = e.msg().to_lowercase();
            msg.contains("disk") && (msg.contains("error") || msg.contains("full") || msg.contains("90%"))
        })
        .count();

    if disk_issues > 0 {
        patterns.push(PatternMatch {
            name: "Disk Issues".to_string(),
            description: format!("{} disk-related issues detected", disk_issues),
            count: disk_issues,
            severity: Severity::Warning,
        });
    }

    // Pattern: Firewall blocks
    let firewall_blocks: usize = entries
        .iter()
        .filter(|e| e.msg().contains("UFW BLOCK") || e.msg().contains("BLOCKED"))
        .count();

    if firewall_blocks >= 2 {
        patterns.push(PatternMatch {
            name: "Firewall Blocks".to_string(),
            description: format!("{} blocked connection attempts", firewall_blocks),
            count: firewall_blocks,
            severity: Severity::Info,
        });
    }

    patterns
}

// ============ Output Functions ============

fn print_summary(result: &AnalysisResult) {
    println!("\n{}", "SUMMARY".bold().underline());
    println!("  Total entries analyzed: {}", result.total_entries.to_string().cyan());

    let errors = result.entries_by_priority.get(&3).unwrap_or(&0);
    let warnings = result.entries_by_priority.get(&4).unwrap_or(&0);
    let critical = result.entries_by_priority.get(&2).unwrap_or(&0)
                 + result.entries_by_priority.get(&1).unwrap_or(&0)
                 + result.entries_by_priority.get(&0).unwrap_or(&0);

    println!("  Critical/Alert/Emerg:   {}", critical.to_string().red().bold());
    println!("  Errors:                 {}", errors.to_string().red());
    println!("  Warnings:               {}", warnings.to_string().yellow());
}

fn print_priority_breakdown(result: &AnalysisResult) {
    println!("\n{}", "PRIORITY DISTRIBUTION".bold().underline());

    let priority_names = [
        (0, "EMERG  "),
        (1, "ALERT  "),
        (2, "CRIT   "),
        (3, "ERR    "),
        (4, "WARNING"),
        (5, "NOTICE "),
        (6, "INFO   "),
        (7, "DEBUG  "),
    ];

    let max_count = result.entries_by_priority.values().max().copied().unwrap_or(1);

    for (priority, name) in priority_names {
        let count = result.entries_by_priority.get(&priority).unwrap_or(&0);
        if *count == 0 {
            continue;
        }

        let bar_width = ((*count as f64 / max_count as f64) * 30.0) as usize;
        let bar = "█".repeat(bar_width);

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

fn print_service_breakdown(result: &AnalysisResult, top: usize) {
    println!("\n{}", "TOP SERVICES".bold().underline());

    let mut services: Vec<(&String, &usize)> = result.entries_by_service.iter().collect();
    services.sort_by(|a, b| b.1.cmp(a.1));
    services.truncate(top);

    let max_count = services.first().map(|(_, c)| **c).unwrap_or(1);

    for (service, count) in services {
        let bar_width = ((*count as f64 / max_count as f64) * 30.0) as usize;
        let bar = "█".repeat(bar_width);
        println!("  {:15} {} {}", service.cyan(), bar.dimmed(), count);
    }
}

fn print_top_errors(result: &AnalysisResult) {
    if result.error_messages.is_empty() {
        return;
    }

    println!("\n{}", "TOP ERROR MESSAGES".bold().underline());

    for (i, (msg, count)) in result.error_messages.iter().enumerate() {
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

fn print_patterns(result: &AnalysisResult) {
    if result.patterns_detected.is_empty() {
        println!("\n{}", "No concerning patterns detected.".green());
        return;
    }

    println!("\n{}", "⚠ PATTERNS DETECTED".bold().underline().yellow());

    for pattern in &result.patterns_detected {
        let severity_icon = match pattern.severity {
            Severity::Critical => "🔴".to_string(),
            Severity::Warning => "🟡".to_string(),
            Severity::Info => "🔵".to_string(),
        };

        let name = match pattern.severity {
            Severity::Critical => pattern.name.red().bold(),
            Severity::Warning => pattern.name.yellow(),
            Severity::Info => pattern.name.blue(),
        };

        println!("  {} {}: {}", severity_icon, name, pattern.description);
    }
}
