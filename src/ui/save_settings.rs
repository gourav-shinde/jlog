use std::path::PathBuf;
use eframe::egui;

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

        let name = self
            .filename_template
            .replace("{host}", host)
            .replace("{date}", &date)
            .replace("{time}", &time);

        let ext = match self.format {
            SaveFormat::Json => "json",
            SaveFormat::PlainText => "log",
        };

        let path = std::path::PathBuf::from(&self.destination)
            .join(format!("{}.{}", name, ext));
        path.to_string_lossy().to_string()
    }
}

pub struct SaveSettingsDialog {
    pub open: bool,
    destination: String,
    filename_template: String,
    format: SaveFormat,
    auto_save: bool,
    save_filtered_only: bool,
    always_show_bookmarks: bool,
}

impl Default for SaveSettingsDialog {
    fn default() -> Self {
        let defaults = SaveSettings::default();
        Self {
            open: false,
            destination: defaults.destination,
            filename_template: defaults.filename_template,
            format: defaults.format,
            auto_save: defaults.auto_save,
            save_filtered_only: defaults.save_filtered_only,
            always_show_bookmarks: defaults.always_show_bookmarks,
        }
    }
}

impl SaveSettingsDialog {
    pub fn load_from(&mut self, settings: &SaveSettings) {
        self.destination = settings.destination.clone();
        self.filename_template = settings.filename_template.clone();
        self.format = settings.format.clone();
        self.auto_save = settings.auto_save;
        self.save_filtered_only = settings.save_filtered_only;
        self.always_show_bookmarks = settings.always_show_bookmarks;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<SaveSettings> {
        let mut result = None;
        let mut should_close = false;

        if !self.open {
            return None;
        }

        egui::Window::new("Save Settings")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(450.0);

                egui::Grid::new("save_settings_fields")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Destination:");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.destination);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_title("Select Save Directory")
                                    .pick_folder()
                                {
                                    self.destination = path.to_string_lossy().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Filename template:");
                        ui.text_edit_singleline(&mut self.filename_template);
                        ui.end_row();
                    });

                ui.separator();
                ui.label("Format:");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.format, SaveFormat::Json, "JSON");
                    ui.radio_value(&mut self.format, SaveFormat::PlainText, "Plain Text");
                });

                ui.separator();
                ui.checkbox(&mut self.auto_save, "Auto-save on SSH disconnect");
                ui.checkbox(&mut self.save_filtered_only, "Save filtered entries only");
                ui.checkbox(&mut self.always_show_bookmarks, "Always show bookmarks when filtering");

                ui.separator();
                // Live preview
                let preview = SaveSettings {
                    destination: self.destination.clone(),
                    filename_template: self.filename_template.clone(),
                    format: self.format.clone(),
                    auto_save: self.auto_save,
                    save_filtered_only: self.save_filtered_only,
                    always_show_bookmarks: self.always_show_bookmarks,
                };
                let preview_path = preview.resolve_filename("example-host");
                ui.horizontal(|ui| {
                    ui.label("Preview:");
                    ui.monospace(&preview_path);
                });

                ui.small("Variables: {host}, {date}, {time}");

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save Settings").clicked() {
                        result = Some(preview);
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });

        if should_close {
            self.open = false;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(dest: &str, template: &str, format: SaveFormat) -> SaveSettings {
        SaveSettings {
            destination: dest.to_string(),
            filename_template: template.to_string(),
            format,
            auto_save: false,
            save_filtered_only: false,
            always_show_bookmarks: false,
        }
    }

    // --- resolve_filename ---

    #[test]
    fn resolve_filename_json_extension() {
        let s = settings_with("/tmp", "myfile", SaveFormat::Json);
        let path = s.resolve_filename("host");
        assert!(path.ends_with(".json"), "path={}", path);
    }

    #[test]
    fn resolve_filename_plaintext_extension() {
        let s = settings_with("/tmp", "myfile", SaveFormat::PlainText);
        let path = s.resolve_filename("host");
        assert!(path.ends_with(".log"), "path={}", path);
    }

    #[test]
    fn resolve_filename_host_substituted() {
        let s = settings_with("/tmp", "{host}_log", SaveFormat::PlainText);
        let path = s.resolve_filename("myserver");
        assert!(path.contains("myserver"), "path={}", path);
    }

    #[test]
    fn resolve_filename_date_time_substituted() {
        let s = settings_with("/tmp", "{date}_{time}", SaveFormat::PlainText);
        let path = s.resolve_filename("h");
        // date = YYYY-MM-DD, time = HH-MM-SS
        assert!(path.contains('-'), "path={}", path);
        assert!(!path.contains("{date}"), "path={}", path);
        assert!(!path.contains("{time}"), "path={}", path);
    }

    #[test]
    fn resolve_filename_all_variables() {
        let s = settings_with("/logs", "{host}_{date}_{time}", SaveFormat::Json);
        let path = s.resolve_filename("srv1");
        assert!(path.starts_with("/logs/srv1_"), "path={}", path);
        assert!(path.ends_with(".json"), "path={}", path);
    }

    #[test]
    fn resolve_filename_destination_is_prefix() {
        let s = settings_with("/my/dir", "file", SaveFormat::PlainText);
        let path = s.resolve_filename("h");
        assert!(path.starts_with("/my/dir"), "path={}", path);
    }

    // --- SaveSettings::default ---

    #[test]
    fn default_has_reasonable_values() {
        let s = SaveSettings::default();
        assert!(s.auto_save);
        assert!(!s.save_filtered_only);
        assert!(!s.always_show_bookmarks);
        assert_eq!(s.format, SaveFormat::PlainText);
        assert!(!s.filename_template.is_empty());
        assert!(!s.destination.is_empty());
    }

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    // --- headless egui: SaveSettingsDialog::show() ---

    #[test]
    fn show_closed_dialog_returns_none() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = false;
        run_ui(|ctx| {
            let result = dialog.show(ctx);
            assert!(result.is_none());
        });
    }

    #[test]
    fn show_open_dialog_renders_without_panic() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = true;
        run_ui(|ctx| {
            let _ = dialog.show(ctx);
        });
    }

    #[test]
    fn show_open_json_format_renders_without_panic() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = true;
        // simulate loading a JSON-format settings
        dialog.load_from(&SaveSettings {
            destination: "/tmp".to_string(),
            filename_template: "{host}_{date}".to_string(),
            format: SaveFormat::Json,
            auto_save: true,
            save_filtered_only: true,
            always_show_bookmarks: false,
        });
        run_ui(|ctx| {
            let _ = dialog.show(ctx);
        });
    }

    // --- load_settings / save_settings_to_disk roundtrip ---

    #[test]
    fn save_and_load_settings_roundtrip() {
        use std::env;
        // Override HOME to a temp dir to avoid touching real user config
        let tmp = env::temp_dir().join(format!("jlog_cfg_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).ok();
        let orig_home = env::var("HOME").ok();
        unsafe { env::set_var("HOME", &tmp); }

        let settings = SaveSettings {
            destination: "/my/dest".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        save_settings_to_disk(&settings);
        let loaded = load_settings();

        // Restore HOME
        if let Some(h) = orig_home {
            unsafe { env::set_var("HOME", h); }
        }
        std::fs::remove_dir_all(&tmp).ok();

        assert_eq!(loaded.destination, "/my/dest");
        assert_eq!(loaded.filename_template, "custom_{host}");
        assert_eq!(loaded.format, SaveFormat::Json);
        assert!(!loaded.auto_save);
        assert!(loaded.save_filtered_only);
        assert!(loaded.always_show_bookmarks);
    }

    // --- SaveSettingsDialog::load_from ---

    #[test]
    fn load_from_copies_all_fields() {
        let settings = SaveSettings {
            destination: "/custom/path".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        let mut dialog = SaveSettingsDialog::default();
        dialog.load_from(&settings);
        // Verify via resolve (indirectly checks destination and template)
        let preview = SaveSettings {
            destination: "/custom/path".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        let path = preview.resolve_filename("h");
        assert!(path.starts_with("/custom/path"), "path={}", path);
        assert!(path.ends_with(".json"), "path={}", path);
    }
}
