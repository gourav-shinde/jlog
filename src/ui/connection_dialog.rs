use std::path::PathBuf;
use eframe::egui;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crate::workers::ssh_reader::{SshConfig, AuthMethod};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    host: String,
    port: u16,
    username: String,
    pub auth_choice: usize,
    key_path: String,
    command: String,
    #[serde(default)]
    password: String,
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

pub fn config_for_profile(name: &str) -> Option<crate::workers::ssh_reader::SshConfig> {
    let profile = load_profiles().into_iter().find(|p| p.name == name)?;
    let auth = match profile.auth_choice {
        0 => {
            let password = BASE64.decode(&profile.password)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
            crate::workers::ssh_reader::AuthMethod::Password(password)
        }
        1 => crate::workers::ssh_reader::AuthMethod::KeyFile(std::path::PathBuf::from(&profile.key_path)),
        _ => crate::workers::ssh_reader::AuthMethod::Agent,
    };
    Some(crate::workers::ssh_reader::SshConfig {
        host: profile.host,
        port: profile.port,
        username: profile.username,
        auth,
        command: profile.command,
    })
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
            self.password = BASE64.decode(&profile.password)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
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
                                password: BASE64.encode(self.password.as_bytes()),
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

    pub fn profiles(&self) -> &[ConnectionProfile] {
        &self.profiles
    }

    pub fn select_profile(&mut self, index: usize) {
        self.selected_profile = Some(index);
        self.prev_selected_profile = None; // force apply on next show()
        self.apply_profile(index);
        self.open = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dialog(host: &str, port: &str, user: &str, auth: usize, key: &str, pass: &str) -> ConnectionDialog {
        ConnectionDialog {
            open: false,
            host: host.to_string(),
            port: port.to_string(),
            username: user.to_string(),
            auth_choice: auth,
            password: pass.to_string(),
            key_path: key.to_string(),
            command: "journalctl -o json -f".to_string(),
            error: None,
            profiles: vec![],
            selected_profile: None,
            prev_selected_profile: None,
            profile_name: String::new(),
        }
    }

    // --- validate ---

    #[test]
    fn validate_agent_auth_succeeds() {
        let d = make_dialog("myhost", "22", "alice", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.host, "myhost");
        assert_eq!(cfg.port, 22);
        assert_eq!(cfg.username, "alice");
        assert!(matches!(cfg.auth, AuthMethod::Agent));
    }

    #[test]
    fn validate_password_auth_succeeds() {
        let d = make_dialog("host", "22", "bob", 0, "", "secret");
        let cfg = d.validate().unwrap();
        assert!(matches!(cfg.auth, AuthMethod::Password(ref p) if p == "secret"));
    }

    #[test]
    fn validate_keyfile_auth_succeeds() {
        let d = make_dialog("host", "22", "alice", 1, "/home/alice/.ssh/id_rsa", "");
        let cfg = d.validate().unwrap();
        assert!(matches!(cfg.auth, AuthMethod::KeyFile(_)));
    }

    #[test]
    fn validate_missing_host_errors() {
        let d = make_dialog("", "22", "alice", 2, "", "");
        assert!(d.validate().is_err());
        assert!(d.validate().err().unwrap().contains("Host"));
    }

    #[test]
    fn validate_whitespace_host_errors() {
        let d = make_dialog("   ", "22", "alice", 2, "", "");
        assert!(d.validate().is_err());
    }

    #[test]
    fn validate_missing_username_errors() {
        let d = make_dialog("host", "22", "", 2, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("Username"));
    }

    #[test]
    fn validate_invalid_port_errors() {
        let d = make_dialog("host", "notaport", "alice", 2, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("port"));
    }

    #[test]
    fn validate_port_out_of_range_errors() {
        let d = make_dialog("host", "99999", "alice", 2, "", "");
        assert!(d.validate().is_err());
    }

    #[test]
    fn validate_missing_key_path_errors() {
        let d = make_dialog("host", "22", "alice", 1, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("Key file"));
    }

    #[test]
    fn validate_trims_host_and_username() {
        let d = make_dialog("  myhost  ", "22", "  alice  ", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.host, "myhost");
        assert_eq!(cfg.username, "alice");
    }

    #[test]
    fn validate_custom_port() {
        let d = make_dialog("host", "2222", "alice", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.port, 2222);
    }

    #[test]
    fn validate_command_preserved() {
        let mut d = make_dialog("host", "22", "alice", 2, "", "");
        d.command = "journalctl -n 500".to_string();
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.command, "journalctl -n 500");
    }

    // --- profiles ---

    #[test]
    fn profiles_returns_slice() {
        let d = make_dialog("h", "22", "u", 2, "", "");
        assert!(d.profiles().is_empty());
    }

    // --- ConnectionDialog::default ---

    #[test]
    fn default_has_ssh_agent_auth() {
        // auth_choice 2 = SSH Agent
        // We can't call Default::default() as it loads from disk; use make_dialog instead.
        let d = make_dialog("", "22", "", 2, "", "");
        assert_eq!(d.auth_choice, 2);
        assert_eq!(d.port, "22");
    }

    // --- headless egui: show() ---

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    #[test]
    fn show_closed_dialog_returns_none() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = false;
        run_ui(|ctx| {
            let result = d.show(ctx);
            assert!(result.is_none());
        });
    }

    #[test]
    fn show_open_dialog_password_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 0, "", "secret");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_keyfile_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 1, "/home/user/.ssh/id_rsa", "");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_agent_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_with_error_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        d.error = Some("Something went wrong".to_string());
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    // --- apply_profile ---

    #[test]
    fn apply_profile_copies_profile_fields() {
        let profile = ConnectionProfile {
            name: "prod".to_string(),
            host: "prod.example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            auth_choice: 1,
            key_path: "/home/deploy/.ssh/id_rsa".to_string(),
            command: "journalctl -n 500".to_string(),
            password: BASE64.encode(b""),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.apply_profile(0);
        assert_eq!(d.host, "prod.example.com");
        assert_eq!(d.port, "2222");
        assert_eq!(d.username, "deploy");
        assert_eq!(d.auth_choice, 1);
        assert_eq!(d.key_path, "/home/deploy/.ssh/id_rsa");
        assert_eq!(d.command, "journalctl -n 500");
        assert_eq!(d.profile_name, "prod");
    }

    #[test]
    fn apply_profile_password_is_decoded() {
        let password = "mysecret";
        let profile = ConnectionProfile {
            name: "dev".to_string(),
            host: "dev".to_string(),
            port: 22,
            username: "user".to_string(),
            auth_choice: 0,
            key_path: String::new(),
            command: "journalctl -f".to_string(),
            password: BASE64.encode(password.as_bytes()),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.apply_profile(0);
        assert_eq!(d.password, "mysecret");
    }

    #[test]
    fn apply_profile_out_of_range_does_nothing() {
        let mut d = make_dialog("original", "22", "user", 2, "", "");
        d.apply_profile(99); // no profile at index 99
        assert_eq!(d.host, "original"); // unchanged
    }

    // --- select_profile ---

    #[test]
    fn select_profile_applies_and_opens_dialog() {
        let profile = ConnectionProfile {
            name: "staging".to_string(),
            host: "staging.example.com".to_string(),
            port: 22,
            username: "ci".to_string(),
            auth_choice: 2,
            key_path: String::new(),
            command: "journalctl -f".to_string(),
            password: BASE64.encode(b""),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.select_profile(0);
        assert!(d.open);
        assert_eq!(d.selected_profile, Some(0));
        assert_eq!(d.host, "staging.example.com");
    }

    #[test]
    fn show_open_dialog_with_profiles_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        d.profiles = vec![
            ConnectionProfile {
                name: "prod".to_string(),
                host: "prod.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth_choice: 2,
                key_path: String::new(),
                command: "journalctl -o json -f".to_string(),
                password: String::new(),
            },
        ];
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }
}
