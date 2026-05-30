use std::collections::HashSet;
use regex::Regex;
use crate::analyzer::state::LogEntry;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombineMode {
    Match,
    And,
    Or,
    Not,
}

pub struct FilterCriteria {
    /// Empty set means "all services". Non-empty means only matching services pass.
    pub units: HashSet<String>,
    pub max_priority: u8,
    pub pattern: Option<Regex>,
    pub pattern2: Option<Regex>,
    pub combine_mode: CombineMode,
}

impl Default for FilterCriteria {
    fn default() -> Self {
        Self {
            units: HashSet::new(),
            max_priority: 7,
            pattern: None,
            pattern2: None,
            combine_mode: CombineMode::Match,
        }
    }
}

impl FilterCriteria {
    /// Check if a LogEntry passes all filters
    pub fn matches(&self, entry: &LogEntry) -> bool {
        if entry.priority > self.max_priority {
            return false;
        }

        if !self.units.is_empty() && !self.units.contains(&entry.service) {
            return false;
        }

        let p1_match = self.pattern.as_ref()
            .map(|r| r.is_match(&entry.message))
            .unwrap_or(true);

        let p2_match = self.pattern2.as_ref()
            .map(|r| r.is_match(&entry.message))
            .unwrap_or(true);

        match self.combine_mode {
            CombineMode::Match => p1_match,
            CombineMode::And => p1_match && p2_match,
            CombineMode::Or => {
                let p1_active = self.pattern.is_some();
                let p2_active = self.pattern2.is_some();
                match (p1_active, p2_active) {
                    (true, true) => p1_match || p2_match,
                    (true, false) => p1_match,
                    (false, true) => p2_match,
                    (false, false) => true,
                }
            }
            CombineMode::Not => {
                self.pattern.as_ref()
                    .map_or(true, |r| !r.is_match(&entry.message))
            }
        }
    }

    pub fn set_pattern(&mut self, text: &str) -> bool {
        if text.is_empty() {
            self.pattern = None;
            return true;
        }
        match Regex::new(text) {
            Ok(r) => { self.pattern = Some(r); true }
            Err(_) => false,
        }
    }

    pub fn set_pattern2(&mut self, text: &str) -> bool {
        if text.is_empty() {
            self.pattern2 = None;
            return true;
        }
        match Regex::new(text) {
            Ok(r) => { self.pattern2 = Some(r); true }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::state::LogEntry;

    fn make_entry(service: &str, priority: u8, message: &str) -> LogEntry {
        LogEntry {
            line_num: 1,
            timestamp: "2026-01-01 00:00:00".to_string(),
            priority,
            service: service.to_string(),
            message: message.to_string(),
        }
    }

    fn default_filter() -> FilterCriteria {
        FilterCriteria::default()
    }

    // --- default ---

    #[test]
    fn default_matches_everything() {
        let f = default_filter();
        assert!(f.matches(&make_entry("sshd", 6, "hello")));
        assert!(f.matches(&make_entry("kernel", 0, "panic")));
    }

    // --- priority filter ---

    #[test]
    fn priority_filter_passes_equal() {
        let mut f = default_filter();
        f.max_priority = 4;
        assert!(f.matches(&make_entry("sshd", 4, "msg")));
    }

    #[test]
    fn priority_filter_passes_lower_number() {
        let mut f = default_filter();
        f.max_priority = 4;
        assert!(f.matches(&make_entry("sshd", 3, "msg")));
    }

    #[test]
    fn priority_filter_rejects_higher_number() {
        let mut f = default_filter();
        f.max_priority = 4;
        assert!(!f.matches(&make_entry("sshd", 5, "msg")));
    }

    // --- service filter ---

    #[test]
    fn service_filter_empty_allows_all() {
        let f = default_filter();
        assert!(f.matches(&make_entry("sshd", 6, "msg")));
        assert!(f.matches(&make_entry("kernel", 6, "msg")));
    }

    #[test]
    fn service_filter_allows_matching_service() {
        let mut f = default_filter();
        f.units.insert("sshd".to_string());
        assert!(f.matches(&make_entry("sshd", 6, "msg")));
    }

    #[test]
    fn service_filter_rejects_non_matching_service() {
        let mut f = default_filter();
        f.units.insert("sshd".to_string());
        assert!(!f.matches(&make_entry("kernel", 6, "msg")));
    }

    #[test]
    fn service_filter_allows_multiple_services() {
        let mut f = default_filter();
        f.units.insert("sshd".to_string());
        f.units.insert("kernel".to_string());
        assert!(f.matches(&make_entry("sshd", 6, "msg")));
        assert!(f.matches(&make_entry("kernel", 6, "msg")));
        assert!(!f.matches(&make_entry("nginx", 6, "msg")));
    }

    // --- pattern / Match mode ---

    #[test]
    fn match_mode_no_pattern_passes_all() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Match;
        assert!(f.matches(&make_entry("sshd", 6, "anything")));
    }

    #[test]
    fn match_mode_with_pattern_filters() {
        let mut f = default_filter();
        f.set_pattern("error");
        assert!(f.matches(&make_entry("sshd", 6, "an error occurred")));
        assert!(!f.matches(&make_entry("sshd", 6, "all good")));
    }

    // --- And mode ---

    #[test]
    fn and_mode_requires_both_patterns() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::And;
        f.set_pattern("error");
        f.set_pattern2("ssh");
        assert!(f.matches(&make_entry("sshd", 6, "ssh error")));
        assert!(!f.matches(&make_entry("sshd", 6, "ssh ok")));
        assert!(!f.matches(&make_entry("sshd", 6, "error elsewhere")));
    }

    #[test]
    fn and_mode_no_patterns_passes_all() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::And;
        assert!(f.matches(&make_entry("sshd", 6, "hello")));
    }

    // --- Or mode ---

    #[test]
    fn or_mode_passes_if_either_matches() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Or;
        f.set_pattern("error");
        f.set_pattern2("warn");
        assert!(f.matches(&make_entry("sshd", 6, "an error")));
        assert!(f.matches(&make_entry("sshd", 6, "a warn")));
        assert!(!f.matches(&make_entry("sshd", 6, "all fine")));
    }

    #[test]
    fn or_mode_only_p1_active() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Or;
        f.set_pattern("error");
        assert!(f.matches(&make_entry("sshd", 6, "error here")));
        assert!(!f.matches(&make_entry("sshd", 6, "nothing")));
    }

    #[test]
    fn or_mode_only_p2_active() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Or;
        f.set_pattern2("warn");
        assert!(f.matches(&make_entry("sshd", 6, "warning issued")));
        assert!(!f.matches(&make_entry("sshd", 6, "nothing")));
    }

    #[test]
    fn or_mode_no_patterns_passes_all() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Or;
        assert!(f.matches(&make_entry("sshd", 6, "anything")));
    }

    // --- Not mode ---

    #[test]
    fn not_mode_rejects_matches() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Not;
        f.set_pattern("error");
        assert!(!f.matches(&make_entry("sshd", 6, "an error")));
        assert!(f.matches(&make_entry("sshd", 6, "all good")));
    }

    #[test]
    fn not_mode_no_pattern_passes_all() {
        let mut f = default_filter();
        f.combine_mode = CombineMode::Not;
        assert!(f.matches(&make_entry("sshd", 6, "anything")));
    }

    // --- set_pattern / set_pattern2 ---

    #[test]
    fn set_pattern_empty_clears() {
        let mut f = default_filter();
        f.set_pattern("error");
        assert!(f.pattern.is_some());
        f.set_pattern("");
        assert!(f.pattern.is_none());
    }

    #[test]
    fn set_pattern_invalid_regex_returns_false() {
        let mut f = default_filter();
        let ok = f.set_pattern("[unclosed");
        assert!(!ok);
    }

    #[test]
    fn set_pattern2_empty_clears() {
        let mut f = default_filter();
        f.set_pattern2("warn");
        assert!(f.pattern2.is_some());
        f.set_pattern2("");
        assert!(f.pattern2.is_none());
    }

    #[test]
    fn set_pattern2_invalid_regex_returns_false() {
        let mut f = default_filter();
        assert!(!f.set_pattern2("(bad"));
    }

    // --- combined filters ---

    #[test]
    fn priority_and_service_and_pattern_all_must_pass() {
        let mut f = default_filter();
        f.max_priority = 4;
        f.units.insert("sshd".to_string());
        f.set_pattern("auth");

        // all conditions met
        assert!(f.matches(&make_entry("sshd", 3, "auth accepted")));
        // wrong service
        assert!(!f.matches(&make_entry("kernel", 3, "auth accepted")));
        // wrong priority
        assert!(!f.matches(&make_entry("sshd", 5, "auth accepted")));
        // wrong message
        assert!(!f.matches(&make_entry("sshd", 3, "connected")));
    }
}
