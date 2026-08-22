use std::path::PathBuf;

/// A regex pattern the user saved under a friendly name for reuse.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NamedPattern {
    pub name: String,
    pub pattern: String,
}

/// The full collection of saved patterns, persisted to disk.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SavedPatterns {
    pub patterns: Vec<NamedPattern>,
}

impl SavedPatterns {
    /// Insert a pattern, replacing any existing entry with the same name.
    pub fn upsert(&mut self, name: &str, pattern: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(existing) = self.patterns.iter_mut().find(|p| p.name == name) {
            existing.pattern = pattern.to_string();
        } else {
            self.patterns.push(NamedPattern {
                name: name.to_string(),
                pattern: pattern.to_string(),
            });
        }
    }

    /// Remove a pattern by name. No-op if it doesn't exist.
    pub fn remove(&mut self, name: &str) {
        self.patterns.retain(|p| p.name != name);
    }
}

fn patterns_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".config").join("jlog").join("patterns.json")
}

pub fn load_patterns() -> SavedPatterns {
    let path = patterns_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_patterns_to_disk(patterns: &SavedPatterns) {
    let path = patterns_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(patterns) {
        let _ = std::fs::write(&path, data);
    }
}

#[cfg(test)]
#[path = "../tests/saved_patterns_tests.rs"]
mod tests;
