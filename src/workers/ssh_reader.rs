use std::io::BufRead;
use std::net::TcpStream;
use std::path::PathBuf;
use crossbeam_channel::{Sender, Receiver};
use ssh2::Session;
use crate::analyzer::LogEntry;
use crate::background::{BackgroundMessage, BackgroundCommand};
use crate::journalctl::JournalEntry;

#[derive(Clone)]
pub enum AuthMethod {
    Password(String),
    KeyFile(PathBuf),
    Agent,
}

#[derive(Clone)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub command: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: AuthMethod::Agent,
            command: "journalctl -o json --no-pager -n 10000 -f".to_string(),
        }
    }
}

pub fn start_ssh(config: SshConfig, tx: Sender<BackgroundMessage>, cmd_rx: Receiver<BackgroundCommand>) {
    std::thread::spawn(move || {
        if let Err(e) = do_ssh(&config, &tx, &cmd_rx) {
            let _ = tx.send(BackgroundMessage::Error(format!("SSH error: {}", e)));
        }
        let _ = tx.send(BackgroundMessage::SshDisconnected);
    });
}

fn do_ssh(config: &SshConfig, tx: &Sender<BackgroundMessage>, cmd_rx: &Receiver<BackgroundCommand>) -> anyhow::Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let tcp = TcpStream::connect(&addr)?;
    tcp.set_nonblocking(false)?;

    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;

    match &config.auth {
        AuthMethod::Password(pw) => {
            session.userauth_password(&config.username, pw)?;
        }
        AuthMethod::KeyFile(path) => {
            session.userauth_pubkey_file(&config.username, None, path, None)?;
        }
        AuthMethod::Agent => {
            session.userauth_agent(&config.username)?;
        }
    }

    if !session.authenticated() {
        return Err(anyhow::anyhow!("Authentication failed"));
    }

    let _ = tx.send(BackgroundMessage::SshConnected);

    let mut channel = session.channel_session()?;
    channel.exec(&config.command)?;

    let reader = std::io::BufReader::new(channel.stream(0));
    let mut lines_read = 0usize;
    let mut entries_sent = 0usize;
    let mut parse_errors = 0usize;

    for line_result in reader.lines() {
        // Check for cancel/disconnect commands (non-blocking)
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                BackgroundCommand::Cancel | BackgroundCommand::Disconnect => {
                    return Ok(());
                }
            }
        }

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                let _ = tx.send(BackgroundMessage::Error(format!("Read error: {}", e)));
                break;
            }
        };

        lines_read += 1;
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        if let Some(entry) = parse_ssh_line(&line, &mut parse_errors) {
            let log_entry = journal_to_log_entry(lines_read, &entry);
            if tx.send(BackgroundMessage::Entry(log_entry)).is_err() {
                return Ok(());
            }
            entries_sent += 1;
        }

        if lines_read % 1000 == 0 {
            let _ = tx.send(BackgroundMessage::Progress {
                lines: lines_read,
                percent: 0.0, // no file size for SSH
            });
        }
    }

    let _ = tx.send(BackgroundMessage::Completed {
        total_lines: lines_read,
        entries: entries_sent,
    });

    Ok(())
}

fn parse_ssh_line(line: &str, parse_errors: &mut usize) -> Option<JournalEntry> {
    if line.starts_with('{') {
        if let Ok(entry) = serde_json::from_str::<JournalEntry>(line) {
            return Some(entry);
        }
    }

    if let Some(entry) = JournalEntry::from_syslog_line(line) {
        return Some(entry);
    }

    *parse_errors += 1;
    None
}

fn journal_to_log_entry(line_num: usize, entry: &JournalEntry) -> LogEntry {
    let timestamp = entry.timestamp_micros()
        .and_then(|us| {
            let secs = us / 1_000_000;
            let nsecs = ((us % 1_000_000) * 1_000) as u32;
            chrono::DateTime::from_timestamp(secs, nsecs).map(|dt| {
                if nsecs == 0 {
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
                }
            })
        })
        .unwrap_or_default();

    LogEntry {
        line_num,
        timestamp,
        priority: entry.priority_num(),
        service: entry.service(),
        message: entry.msg().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SshConfig::default ---

    #[test]
    fn ssh_config_default_has_port_22() {
        let cfg = SshConfig::default();
        assert_eq!(cfg.port, 22);
        assert!(cfg.host.is_empty());
        assert!(cfg.username.is_empty());
        assert!(!cfg.command.is_empty());
        assert!(matches!(cfg.auth, AuthMethod::Agent));
    }

    // --- parse_ssh_line ---

    #[test]
    fn parse_ssh_line_valid_json() {
        let json = r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","SYSLOG_IDENTIFIER":"sshd","MESSAGE":"accepted"}"#;
        let e = parse_ssh_line(json, &mut 0).unwrap();
        assert_eq!(e.service(), "sshd");
        assert_eq!(e.msg(), "accepted");
    }

    #[test]
    fn parse_ssh_line_syslog_format() {
        let line = "May 29 10:30:45 host sshd[22]: Accepted publickey";
        let e = parse_ssh_line(line, &mut 0).unwrap();
        assert_eq!(e.service(), "sshd");
    }

    #[test]
    fn parse_ssh_line_garbage_returns_none_and_increments_errors() {
        let mut errs = 0usize;
        assert!(parse_ssh_line("not a log line", &mut errs).is_none());
        assert_eq!(errs, 1);
    }

    #[test]
    fn parse_ssh_line_empty_returns_none() {
        let mut errs = 0usize;
        // empty string doesn't start with '{' and fails syslog parse
        assert!(parse_ssh_line("", &mut errs).is_none());
        assert_eq!(errs, 1);
    }

    // --- journal_to_log_entry ---

    #[test]
    fn journal_to_log_entry_with_microseconds() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: Some("1700000000123456".to_string()),
            priority: Some("4".to_string()),
            syslog_identifier: Some("nginx".to_string()),
            systemd_unit: None,
            message: Some("warn".to_string()),
        };
        let log = journal_to_log_entry(1, &entry);
        assert_eq!(log.service, "nginx");
        assert_eq!(log.priority, 4);
        assert!(log.timestamp.contains('.'), "expected microseconds: {}", log.timestamp);
    }

    #[test]
    fn journal_to_log_entry_whole_seconds() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: Some("1700000000000000".to_string()),
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("ok".to_string()),
        };
        let log = journal_to_log_entry(2, &entry);
        assert!(!log.timestamp.contains('.'), "unexpected microseconds: {}", log.timestamp);
    }

    #[test]
    fn journal_to_log_entry_no_timestamp() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: None,
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("msg".to_string()),
        };
        let log = journal_to_log_entry(3, &entry);
        assert!(log.timestamp.is_empty());
        assert_eq!(log.line_num, 3);
    }

    // --- AuthMethod constructors ---

    #[test]
    fn auth_method_variants_can_be_constructed() {
        let _pw = AuthMethod::Password("secret".to_string());
        let _kf = AuthMethod::KeyFile(std::path::PathBuf::from("/home/user/.ssh/id_rsa"));
        let _ag = AuthMethod::Agent;
    }

    // --- SshConfig clone ---

    #[test]
    fn ssh_config_can_be_cloned() {
        let cfg = SshConfig {
            host: "myhost".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
            command: "journalctl -f".to_string(),
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.host, "myhost");
        assert_eq!(cloned.port, 2222);
    }
}
