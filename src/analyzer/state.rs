use std::collections::HashMap;
use crate::journalctl::JournalEntry;
use super::filter::NormalizeRegex;

/// Time bucket for time-series data
#[derive(Clone, Default)]
pub struct TimeBucket {
    pub total: usize,
    pub errors: usize,   // priority 0-3
    pub warnings: usize, // priority 4
}

/// Streaming analysis state - accumulates statistics without storing entries
pub struct AnalysisState {
    pub total_entries: usize,
    pub entries_by_priority: [usize; 8],
    pub entries_by_service: HashMap<String, usize>,
    pub error_messages: HashMap<String, usize>,

    // Time-series data (minute buckets for flexible aggregation)
    pub time_series: HashMap<String, TimeBucket>,

    // Message trends over time: message -> (minute_bucket -> count)
    pub message_trends: HashMap<String, HashMap<String, usize>>,
}

impl AnalysisState {
    pub fn new() -> Self {
        Self {
            total_entries: 0,
            entries_by_priority: [0; 8],
            entries_by_service: HashMap::new(),
            error_messages: HashMap::new(),
            time_series: HashMap::new(),
            message_trends: HashMap::new(),
        }
    }

    /// Process a single entry - called for each line during streaming
    pub fn process_entry(&mut self, entry: &JournalEntry, normalize_regex: &NormalizeRegex) {
        self.total_entries += 1;

        let priority = entry.priority_num() as usize;
        if priority < 8 {
            self.entries_by_priority[priority] += 1;
        }

        // Count by service
        *self.entries_by_service.entry(entry.service()).or_insert(0) += 1;

        // Track time-series data (at minute granularity for flexible aggregation)
        if let Some(bucket_key) = entry.minute_bucket() {
            let bucket = self.time_series.entry(bucket_key).or_default();
            bucket.total += 1;
            if priority <= 3 {
                bucket.errors += 1;
            } else if priority == 4 {
                bucket.warnings += 1;
            }
        }

        // Track error messages (priority <= 4: warning and above)
        if priority <= 4 {
            let msg = normalize_regex.normalize(entry.msg());
            if !msg.is_empty() {
                *self.error_messages.entry(msg.clone()).or_insert(0) += 1;

                // Track message trend over time (at minute granularity)
                if let Some(bucket_key) = entry.minute_bucket() {
                    let msg_buckets = self.message_trends.entry(msg).or_insert_with(HashMap::new);
                    *msg_buckets.entry(bucket_key).or_insert(0) += 1;
                }
            }
        }
    }

    /// Get top N error messages sorted by count
    pub fn top_errors(&self, n: usize) -> Vec<(String, usize)> {
        let mut errors: Vec<_> = self.error_messages.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        errors.sort_by(|a, b| b.1.cmp(&a.1));
        errors.truncate(n);
        errors
    }

    /// Get top N services sorted by count
    pub fn top_services(&self, n: usize) -> Vec<(String, usize)> {
        let mut services: Vec<_> = self.entries_by_service.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        services.sort_by(|a, b| b.1.cmp(&a.1));
        services.truncate(n);
        services
    }

    /// Get time-series data sorted by time
    pub fn sorted_time_series(&self) -> Vec<(String, TimeBucket)> {
        let mut series: Vec<_> = self.time_series.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        series.sort_by(|a, b| a.0.cmp(&b.0));
        series
    }

    /// Get message trends for top N messages, returns (message, sorted_time_buckets with counts)
    pub fn top_message_trends(&self, n: usize) -> Vec<(String, Vec<(String, usize)>)> {
        let top_msgs = self.top_errors(n);

        top_msgs.into_iter().map(|(msg, _total)| {
            let trend_data = self.message_trends.get(&msg)
                .map(|buckets| {
                    let mut sorted: Vec<_> = buckets.iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    sorted.sort_by(|a, b| a.0.cmp(&b.0));
                    sorted
                })
                .unwrap_or_default();
            (msg, trend_data)
        }).collect()
    }

    /// Dynamically detect patterns by analyzing message frequency and distribution
    pub fn get_patterns(&self) -> Vec<PatternInfo> {
        let mut patterns = Vec::new();
        let all_buckets = self.sorted_time_series();
        let num_buckets = all_buckets.len();

        if num_buckets == 0 {
            return patterns;
        }

        // Analyze each error message for patterns
        for (msg, total_count) in self.error_messages.iter() {
            if let Some(buckets) = self.message_trends.get(msg) {
                let bucket_counts: Vec<usize> = buckets.values().cloned().collect();
                let num_active_buckets = bucket_counts.len();

                if num_active_buckets == 0 {
                    continue;
                }

                // Calculate statistics
                let avg = *total_count as f64 / num_buckets as f64;
                let max_in_bucket = bucket_counts.iter().max().cloned().unwrap_or(0);

                // Detect SPIKE: max bucket value is significantly higher than average
                // A spike means the message occurred much more frequently in one time period
                if num_active_buckets >= 2 && max_in_bucket as f64 > avg * 3.0 && max_in_bucket >= 3 {
                    let spike_bucket = buckets.iter()
                        .max_by_key(|(_, c)| *c)
                        .map(|(t, _)| t.clone())
                        .unwrap_or_default();

                    patterns.push(PatternInfo {
                        pattern_type: PatternType::Spike,
                        message: truncate_msg(msg, 80),
                        description: format!("Spike of {} at {}, avg {:.1}/bucket", max_in_bucket, spike_bucket, avg),
                        severity: if max_in_bucket >= 50 { Severity::Critical } else { Severity::Warning },
                        count: *total_count,
                        details: Some(format!("Peak: {} occurrences in single minute", max_in_bucket)),
                    });
                }

                // Detect BURST: many occurrences concentrated in very few buckets
                // High concentration means the message appeared in bursts rather than spread out
                let concentration = num_active_buckets as f64 / num_buckets as f64;
                if *total_count >= 5 && concentration < 0.3 && num_active_buckets <= 5 {
                    patterns.push(PatternInfo {
                        pattern_type: PatternType::Burst,
                        message: truncate_msg(msg, 80),
                        description: format!("{} occurrences in only {} time windows", total_count, num_active_buckets),
                        severity: Severity::Warning,
                        count: *total_count,
                        details: Some(format!("Concentrated burst - {}% of time range", (concentration * 100.0) as usize)),
                    });
                }

                // Detect RECURRING: message appears consistently across many buckets
                // This indicates a persistent issue that keeps happening
                if *total_count >= 5 && concentration > 0.4 && num_active_buckets >= 3 {
                    patterns.push(PatternInfo {
                        pattern_type: PatternType::Recurring,
                        message: truncate_msg(msg, 80),
                        description: format!("Recurring {} times across {}% of time range", total_count, (concentration * 100.0) as usize),
                        severity: Severity::Warning,
                        count: *total_count,
                        details: Some(format!("Persistent issue - appears in {} buckets", num_active_buckets)),
                    });
                }

                // Detect INCREASING: rate is higher in second half than first half
                // This indicates a growing problem
                if num_active_buckets >= 4 {
                    let sorted_buckets: Vec<_> = {
                        let mut v: Vec<_> = buckets.iter().collect();
                        v.sort_by(|a, b| a.0.cmp(b.0));
                        v
                    };
                    let mid = sorted_buckets.len() / 2;
                    let first_half: usize = sorted_buckets[..mid].iter().map(|(_, c)| *c).sum();
                    let second_half: usize = sorted_buckets[mid..].iter().map(|(_, c)| *c).sum();

                    if second_half > first_half * 2 && second_half >= 5 {
                        patterns.push(PatternInfo {
                            pattern_type: PatternType::Increasing,
                            message: truncate_msg(msg, 80),
                            description: format!("Rate increased from {} to {} ({}x)", first_half, second_half, second_half / first_half.max(1)),
                            severity: Severity::Warning,
                            count: *total_count,
                            details: Some("Frequency increasing over time".to_string()),
                        });
                    }
                }
            }
        }

        // Detect HIGH VOLUME: messages that dominate the error log
        let total_errors: usize = self.error_messages.values().sum();
        if total_errors > 0 {
            for (msg, count) in self.error_messages.iter() {
                let percentage = (*count as f64 / total_errors as f64) * 100.0;
                if percentage > 25.0 && *count >= 5 {
                    // Check if already added as another pattern type
                    let already_added = patterns.iter().any(|p| p.message == truncate_msg(msg, 80));
                    if !already_added {
                        patterns.push(PatternInfo {
                            pattern_type: PatternType::HighVolume,
                            message: truncate_msg(msg, 80),
                            description: format!("{:.1}% of all errors ({} occurrences)", percentage, count),
                            severity: if percentage > 50.0 { Severity::Critical } else { Severity::Warning },
                            count: *count,
                            details: Some(format!("Dominates error log with {:.1}% share", percentage)),
                        });
                    }
                }
            }
        }

        // Sort patterns by severity and count
        patterns.sort_by(|a, b| {
            let sev_ord = severity_order(a.severity).cmp(&severity_order(b.severity));
            if sev_ord != std::cmp::Ordering::Equal {
                sev_ord
            } else {
                b.count.cmp(&a.count)
            }
        });

        // Limit to top 10 patterns
        patterns.truncate(10);
        patterns
    }
}

fn truncate_msg(msg: &str, max_len: usize) -> String {
    if msg.len() <= max_len {
        msg.to_string()
    } else {
        format!("{}...", &msg[..max_len])
    }
}

fn severity_order(s: Severity) -> usize {
    match s {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PatternType {
    Spike,      // Sudden increase in frequency
    Burst,      // Concentrated occurrences in short time
    Recurring,  // Consistent appearance over time
    Increasing, // Rate growing over time
    HighVolume, // Dominates the error log
}

impl PatternType {
    pub fn label(&self) -> &'static str {
        match self {
            PatternType::Spike => "Spike",
            PatternType::Burst => "Burst",
            PatternType::Recurring => "Recurring",
            PatternType::Increasing => "Increasing",
            PatternType::HighVolume => "High Volume",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            PatternType::Spike => "📈",
            PatternType::Burst => "💥",
            PatternType::Recurring => "🔄",
            PatternType::Increasing => "📊",
            PatternType::HighVolume => "🔥",
        }
    }
}

pub struct PatternInfo {
    pub pattern_type: PatternType,
    pub message: String,
    pub description: String,
    pub severity: Severity,
    pub count: usize,
    pub details: Option<String>,
}
