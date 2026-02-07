use crate::analyzer::LogEntry;

pub enum BackgroundMessage {
    Entry(LogEntry),
    Progress { lines: usize, percent: f32 },
    Completed { total_lines: usize, entries: usize },
    Error(String),
    SshConnected,
    SshDisconnected,
}

pub enum BackgroundCommand {
    Cancel,
    Disconnect,
}
