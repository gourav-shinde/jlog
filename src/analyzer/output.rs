use colored::Colorize;
use super::state::{AnalysisState, Severity};

pub fn print_summary(state: &AnalysisState, lines_read: usize) {
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

pub fn print_priority_breakdown(state: &AnalysisState) {
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

pub fn print_service_breakdown(state: &AnalysisState, top: usize) {
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

pub fn print_top_errors(state: &AnalysisState, top: usize) {
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

pub fn print_time_series(state: &AnalysisState) {
    let series = state.sorted_time_series();
    if series.is_empty() {
        return;
    }

    println!("\n{}", "LOG VOLUME OVER TIME".bold().underline());

    let max_count = series.iter().map(|(_, b)| b.total).max().unwrap_or(1);

    for (time, bucket) in series.iter().take(24) {
        let bar_width = ((bucket.total as f64 / max_count as f64) * 30.0) as usize;
        let bar = "█".repeat(bar_width.max(1));

        let bar_colored = if bucket.errors > 0 {
            bar.red()
        } else if bucket.warnings > 0 {
            bar.yellow()
        } else {
            bar.normal()
        };

        println!("  {} {} {} (err:{}, warn:{})",
            time.dimmed(),
            bar_colored,
            bucket.total,
            bucket.errors.to_string().red(),
            bucket.warnings.to_string().yellow()
        );
    }
}

pub fn print_patterns(state: &AnalysisState) {
    let patterns = state.get_patterns();

    if patterns.is_empty() {
        println!("\n{}", "No concerning patterns detected.".green());
        return;
    }

    println!("\n{}", "⚠ DYNAMIC PATTERNS DETECTED".bold().underline().yellow());

    for p in patterns {
        let severity_icon = match p.severity {
            Severity::Critical => "🔴",
            Severity::Warning => "🟡",
            Severity::Info => "🔵",
        };

        let type_label = format!("[{}]", p.pattern_type.label());
        let colored_type = match p.severity {
            Severity::Critical => type_label.red().bold(),
            Severity::Warning => type_label.yellow(),
            Severity::Info => type_label.blue(),
        };

        // Truncate message for display
        let msg_display = if p.message.len() > 50 {
            format!("{}...", &p.message[..47])
        } else {
            p.message.clone()
        };

        println!("  {} {} {}", severity_icon, colored_type, p.description);
        println!("      {}", msg_display.dimmed());
    }
}
