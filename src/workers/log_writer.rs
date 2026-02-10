use std::io::Write;

use crate::analyzer::LogEntry;
use crate::ui::save_settings::{SaveFormat, SaveSettings};

pub fn save_logs(
    entries: &[&LogEntry],
    settings: &SaveSettings,
    host: &str,
) -> anyhow::Result<String> {
    let path = settings.resolve_filename(host);

    // Create destination directory
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(&path)?;

    match settings.format {
        SaveFormat::Json => {
            for entry in entries {
                let obj = serde_json::json!({
                    "line": entry.line_num,
                    "timestamp": entry.timestamp,
                    "priority": entry.priority,
                    "service": entry.service,
                    "message": entry.message,
                });
                serde_json::to_writer(&mut file, &obj)?;
                writeln!(file)?;
            }
        }
        SaveFormat::PlainText => {
            for entry in entries {
                writeln!(
                    file,
                    "{} {}[{}]: {}",
                    entry.timestamp, entry.service, entry.priority, entry.message
                )?;
            }
        }
    }

    Ok(path)
}
