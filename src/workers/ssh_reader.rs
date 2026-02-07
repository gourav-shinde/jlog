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
    let timestamp = entry.timestamp_secs()
        .and_then(|secs| {
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
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
