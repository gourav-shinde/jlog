use std::path::PathBuf;
use eframe::egui;
use crate::workers::ssh_reader::{SshConfig, AuthMethod};

pub struct ConnectionDialog {
    pub open: bool,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_choice: usize, // 0=Password, 1=KeyFile, 2=Agent
    pub password: String,
    pub key_path: String,
    pub command: String,
    pub error: Option<String>,
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        Self {
            open: false,
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_choice: 2, // SSH Agent by default
            password: String::new(),
            key_path: String::new(),
            command: "journalctl -o json --no-pager -n 10000 -f".to_string(),
            error: None,
        }
    }
}

impl ConnectionDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<SshConfig> {
        let mut result = None;
        let mut should_close = false;

        if !self.open {
            return None;
        }

        egui::Window::new("SSH Connection")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(400.0);

                egui::Grid::new("ssh_fields")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Host:");
                        ui.text_edit_singleline(&mut self.host);
                        ui.end_row();

                        ui.label("Port:");
                        ui.text_edit_singleline(&mut self.port);
                        ui.end_row();

                        ui.label("Username:");
                        ui.text_edit_singleline(&mut self.username);
                        ui.end_row();
                    });

                ui.separator();
                ui.label("Authentication:");

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.auth_choice, 0, "Password");
                    ui.radio_value(&mut self.auth_choice, 1, "Key File");
                    ui.radio_value(&mut self.auth_choice, 2, "SSH Agent");
                });

                match self.auth_choice {
                    0 => {
                        ui.horizontal(|ui| {
                            ui.label("Password:");
                            ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                        });
                    }
                    1 => {
                        ui.horizontal(|ui| {
                            ui.label("Key File:");
                            ui.text_edit_singleline(&mut self.key_path);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_title("Select SSH Key")
                                    .pick_file()
                                {
                                    self.key_path = path.to_string_lossy().to_string();
                                }
                            }
                        });
                    }
                    _ => {} // Agent needs no extra fields
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Command:");
                });
                ui.text_edit_singleline(&mut self.command);

                if let Some(ref err) = self.error {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Connect").clicked() {
                        match self.validate() {
                            Ok(config) => {
                                result = Some(config);
                                should_close = true;
                            }
                            Err(e) => {
                                self.error = Some(e);
                            }
                        }
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

    fn validate(&self) -> Result<SshConfig, String> {
        if self.host.trim().is_empty() {
            return Err("Host is required".to_string());
        }
        if self.username.trim().is_empty() {
            return Err("Username is required".to_string());
        }
        let port: u16 = self.port.parse().map_err(|_| "Invalid port number".to_string())?;

        let auth = match self.auth_choice {
            0 => AuthMethod::Password(self.password.clone()),
            1 => {
                if self.key_path.trim().is_empty() {
                    return Err("Key file path is required".to_string());
                }
                AuthMethod::KeyFile(PathBuf::from(&self.key_path))
            }
            _ => AuthMethod::Agent,
        };

        Ok(SshConfig {
            host: self.host.trim().to_string(),
            port,
            username: self.username.trim().to_string(),
            auth,
            command: self.command.clone(),
        })
    }
}
