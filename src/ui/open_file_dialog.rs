use eframe::egui;

pub struct OpenFileDialog {
    pub open: bool,
    pub path: String,
    pub error: Option<String>,
}

impl Default for OpenFileDialog {
    fn default() -> Self {
        Self {
            open: false,
            path: String::new(),
            error: None,
        }
    }
}

impl OpenFileDialog {
    /// Show the dialog. Returns Some(path) when user clicks Open with a valid path.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.open {
            return None;
        }

        let mut result = None;
        let mut should_close = false;

        egui::Window::new("Open Log File")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(500.0);

                ui.label("Enter the path to a log file:");
                ui.add_space(4.0);

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.path)
                        .desired_width(480.0)
                        .hint_text("/var/log/syslog"),
                );

                // Auto-focus the text field when dialog opens
                if resp.gained_focus() || self.error.is_none() {
                    resp.request_focus();
                }

                // Enter key submits
                let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if let Some(ref err) = self.error {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.add_space(8.0);

                // Try native dialog as secondary option
                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Open Log File")
                            .add_filter("Log files", &["log", "txt", "json"])
                            .add_filter("All files", &["*"])
                            .pick_file()
                        {
                            self.path = path.to_string_lossy().to_string();
                        }
                    }
                    ui.label("(may not work on all systems)");
                });

                ui.add_space(4.0);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() || enter_pressed {
                        let path = self.path.trim().to_string();
                        if path.is_empty() {
                            self.error = Some("Path is required".to_string());
                        } else if !std::path::Path::new(&path).exists() {
                            self.error = Some(format!("File not found: {}", path));
                        } else {
                            result = Some(path);
                            should_close = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });

        if should_close {
            self.open = false;
            self.error = None;
        }

        result
    }
}
