use eframe::egui;

#[derive(Clone, Debug, PartialEq)]
pub enum SaveFormat {
    Json,
    PlainText,
}

#[derive(Clone, Debug)]
pub struct SaveSettings {
    pub destination: String,
    pub filename_template: String,
    pub format: SaveFormat,
    pub auto_save: bool,
    pub save_filtered_only: bool,
}

impl Default for SaveSettings {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self {
            destination: format!("{}/logs", home),
            filename_template: "{host}_{date}_{time}".to_string(),
            format: SaveFormat::Json,
            auto_save: true,
            save_filtered_only: false,
        }
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

        format!("{}/{}.{}", self.destination, name, ext)
    }
}

pub struct SaveSettingsDialog {
    pub open: bool,
    destination: String,
    filename_template: String,
    format: SaveFormat,
    auto_save: bool,
    save_filtered_only: bool,
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

                ui.separator();
                // Live preview
                let preview = SaveSettings {
                    destination: self.destination.clone(),
                    filename_template: self.filename_template.clone(),
                    format: self.format.clone(),
                    auto_save: self.auto_save,
                    save_filtered_only: self.save_filtered_only,
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
