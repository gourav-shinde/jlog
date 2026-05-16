use std::path::PathBuf;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crate::workers::ssh_reader::{AuthMethod, SshConfig};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_choice: usize,
    pub key_path: String,
    pub command: String,
    #[serde(default)]
    pub password: String,
}

fn profiles_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".config").join("jlog").join("profiles.json")
}

pub fn load_profiles() -> Vec<ConnectionProfile> {
    let path = profiles_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_profiles(profiles: &[ConnectionProfile]) {
    let path = profiles_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(profiles) {
        let _ = std::fs::write(&path, data);
    }
}

pub fn config_for_profile(name: &str) -> Option<SshConfig> {
    let profile = load_profiles().into_iter().find(|p| p.name == name)?;
    let auth = match profile.auth_choice {
        0 => {
            let password = BASE64.decode(&profile.password)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            AuthMethod::Password(password)
        }
        1 => AuthMethod::KeyFile(std::path::PathBuf::from(&profile.key_path)),
        _ => AuthMethod::Agent,
    };
    Some(SshConfig {
        host: profile.host,
        port: profile.port,
        username: profile.username,
        auth,
        command: profile.command,
    })
}
