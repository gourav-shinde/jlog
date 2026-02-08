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
                if self.pattern.is_some() {
                    !self.pattern.as_ref().unwrap().is_match(&entry.message)
                } else {
                    true
                }
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
