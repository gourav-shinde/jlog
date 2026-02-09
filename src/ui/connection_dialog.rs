use std::path::PathBuf;
use eframe::egui;
use crate::workers::ssh_reader::{SshConfig, AuthMethod};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ConnectionProfile {
    name: String,
    host: String,
    port: u16,
    username: String,
    auth_choice: usize,
    key_path: String,
    command: String,
}

fn profiles_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".config").join("jlog").join("profiles.json")
}

fn load_profiles() -> Vec<ConnectionProfile> {
    let path = profiles_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_profiles(profiles: &[ConnectionProfile]) {
    let path = profiles_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(profiles) {
        let _ = std::fs::write(&path, data);
    }
}

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
    profiles: Vec<ConnectionProfile>,
    selected_profile: Option<usize>,
    prev_selected_profile: Option<usize>,
    profile_name: String,
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
            profiles: load_profiles(),
            selected_profile: None,
            prev_selected_profile: None,
            profile_name: String::new(),
        }
    }
}

impl ConnectionDialog {
    fn apply_profile(&mut self, index: usize) {
        if let Some(profile) = self.profiles.get(index) {
            self.host = profile.host.clone();
            self.port = profile.port.to_string();
            self.username = profile.username.clone();
            self.auth_choice = profile.auth_choice;
            self.key_path = profile.key_path.clone();
            self.command = profile.command.clone();
            self.password = String::new();
            self.profile_name = profile.name.clone();
            self.error = None;
        }
    }

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

                // Profile selector
                ui.horizontal(|ui| {
                    ui.label("Profile:");
                    let current_label = match self.selected_profile {
                        Some(i) => self.profiles.get(i)
                            .map(|p| p.name.as_str())
                            .unwrap_or("New Connection"),
                        None => "New Connection",
                    };
                    egui::ComboBox::from_id_salt("profile_selector")
                        .selected_text(current_label)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            let mut new_selection = self.selected_profile;
                            if ui.selectable_value(&mut new_selection, None, "New Connection").clicked() {
                                // handled below
                            }
                            for (i, profile) in self.profiles.iter().enumerate() {
                                if ui.selectable_value(&mut new_selection, Some(i), &profile.name).clicked() {
                                    // handled below
                                }
                            }
                            if new_selection != self.selected_profile {
                                self.selected_profile = new_selection;
                            }
                        });
                });

                // Detect profile change and apply
                if self.selected_profile != self.prev_selected_profile {
                    self.prev_selected_profile = self.selected_profile;
                    if let Some(idx) = self.selected_profile {
                        self.apply_profile(idx);
                    }
                }

                // Save / Delete buttons
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.profile_name);
                    if ui.button("Save Profile").clicked() {
                        if self.profile_name.trim().is_empty() {
                            self.error = Some("Profile name is required".to_string());
                        } else {
                            let port = self.port.parse().unwrap_or(22);
                            let profile = ConnectionProfile {
                                name: self.profile_name.trim().to_string(),
                                host: self.host.clone(),
                                port,
                                username: self.username.clone(),
                                auth_choice: self.auth_choice,
                                key_path: self.key_path.clone(),
                                command: self.command.clone(),
                            };
                            // Update existing or add new
                            if let Some(pos) = self.profiles.iter().position(|p| p.name == profile.name) {
                                self.profiles[pos] = profile;
                                self.selected_profile = Some(pos);
                            } else {
                                self.profiles.push(profile);
                                self.selected_profile = Some(self.profiles.len() - 1);
                            }
                            save_profiles(&self.profiles);
                            self.error = None;
                        }
                    }
                    if self.selected_profile.is_some() {
                        if ui.button("Delete").clicked() {
                            if let Some(idx) = self.selected_profile {
                                self.profiles.remove(idx);
                                save_profiles(&self.profiles);
                                self.selected_profile = None;
                                self.profile_name = String::new();
                            }
                        }
                    }
                });

                ui.separator();

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
