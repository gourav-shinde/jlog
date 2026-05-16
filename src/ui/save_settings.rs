use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SaveFormat {
    Json,
    PlainText,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SaveSettings {
    pub destination: String,
    pub filename_template: String,
    pub format: SaveFormat,
    pub auto_save: bool,
    pub save_filtered_only: bool,
    pub always_show_bookmarks: bool,
}

impl Default for SaveSettings {
    fn default() -> Self {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        let dest = std::path::PathBuf::from(&home).join("logs");
        Self {
            destination: dest.to_string_lossy().to_string(),
            filename_template: "{host}_{date}_{time}".to_string(),
            format: SaveFormat::PlainText,
            auto_save: true,
            save_filtered_only: false,
            always_show_bookmarks: false,
        }
    }
}

fn settings_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".config").join("jlog").join("settings.json")
}

pub fn load_settings() -> SaveSettings {
    let path = settings_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_settings_to_disk(settings: &SaveSettings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, data);
    }
}

impl SaveSettings {
    pub fn resolve_filename(&self, host: &str) -> String {
        let now = chrono::Local::now();
        let date = now.format("%Y-%m-%d").to_string();
        let time = now.format("%H-%M-%S").to_string();

        let name = self.filename_template
            .replace("{host}", host)
            .replace("{date}", &date)
            .replace("{time}", &time);

        let ext = match self.format {
            SaveFormat::Json => "json",
            SaveFormat::PlainText => "log",
        };

        std::path::PathBuf::from(&self.destination)
            .join(format!("{}.{}", name, ext))
            .to_string_lossy()
            .to_string()
    }
}
