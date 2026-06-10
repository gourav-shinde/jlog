use std::io::BufRead;
use std::net::TcpStream;
use std::path::PathBuf;
use crossbeam_channel::{Sender, Receiver};
use ssh2::Session;
use crate::background::{BackgroundMessage, BackgroundCommand};
use crate::workers::parser::parse_log_line;

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

        // Shared parser handles every format and keeps unrecognized lines as
        // raw entries; it only returns None for blank lines.
        if let Some(log_entry) = parse_log_line(&line, lines_read) {
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

#[cfg(test)]
#[path = "../tests/ssh_reader_tests.rs"]
mod tests;
