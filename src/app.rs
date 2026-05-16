use std::collections::HashSet;
use std::time::Duration;
use crossbeam_channel::{Receiver, Sender};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::analyzer::{filter::FilterCriteria, state::{LogEntry, LogStore}};
use crate::background::{BackgroundCommand, BackgroundMessage};
use crate::ui::profiles::{config_for_profile, ConnectionProfile, load_profiles, save_profiles};
use crate::ui::save_settings::{load_settings, SaveSettings};
use crate::workers::{file_reader, ssh_reader};
use crate::workers::ssh_reader::{AuthMethod, SshConfig};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Filter,
    Find,
    Connect,
    Bookmarks,
    Help,
}

pub struct ConnectForm {
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub key_path: String,
    pub command: String,
    pub auth_choice: usize,   // 0=password, 1=key, 2=agent
    pub focused: usize,       // 0..=6 fields + buttons
    pub profiles: Vec<ConnectionProfile>,
    pub profile_idx: Option<usize>,
    pub profile_name: String,
    pub error: String,
}

impl Default for ConnectForm {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            password: String::new(),
            key_path: String::new(),
            command: "journalctl -o json --no-pager -n 10000 -f".to_string(),
            auth_choice: 0,
            focused: 0,
            profiles: load_profiles(),
            profile_idx: None,
            profile_name: String::new(),
            error: String::new(),
        }
    }
}

impl ConnectForm {
    pub fn field_count(&self) -> usize {
        // host, port, user, auth_choice, password/keypath, command, profile_name = 7 inputs + connect + save + cancel = 10
        10
    }

    pub fn load_profile(&mut self, idx: usize) {
        if let Some(p) = self.profiles.get(idx) {
            self.host = p.host.clone();
            self.port = p.port.to_string();
            self.username = p.username.clone();
            self.auth_choice = p.auth_choice;
            self.key_path = p.key_path.clone();
            self.command = p.command.clone();
            self.password = BASE64.decode(&p.password)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
            self.profile_name = p.name.clone();
            self.profile_idx = Some(idx);
        }
    }

    pub fn to_ssh_config(&self) -> Result<SshConfig, String> {
        if self.host.is_empty() {
            return Err("Host is required".to_string());
        }
        let port: u16 = self.port.parse().map_err(|_| "Invalid port".to_string())?;
        let auth = match self.auth_choice {
            0 => AuthMethod::Password(self.password.clone()),
            1 => {
                if self.key_path.is_empty() {
                    return Err("Key path is required".to_string());
                }
                AuthMethod::KeyFile(std::path::PathBuf::from(&self.key_path))
            }
            _ => AuthMethod::Agent,
        };
        Ok(SshConfig {
            host: self.host.clone(),
            port,
            username: self.username.clone(),
            auth,
            command: self.command.clone(),
        })
    }

    pub fn save_current_profile(&mut self) {
        if self.profile_name.is_empty() {
            return;
        }
        let profile = ConnectionProfile {
            name: self.profile_name.clone(),
            host: self.host.clone(),
            port: self.port.parse().unwrap_or(22),
            username: self.username.clone(),
            auth_choice: self.auth_choice,
            key_path: self.key_path.clone(),
            command: self.command.clone(),
            password: BASE64.encode(self.password.as_bytes()),
        };
        if let Some(idx) = self.profiles.iter().position(|p| p.name == profile.name) {
            self.profiles[idx] = profile;
        } else {
            self.profiles.push(profile);
        }
        save_profiles(&self.profiles);
    }
}

pub struct App {
    // Data
    pub log_store: LogStore,
    pub filter: FilterCriteria,
    pub filtered_indices: Vec<usize>,
    pub bookmarks: HashSet<usize>,

    // Background
    bg_receiver: Option<Receiver<BackgroundMessage>>,
    bg_cmd_sender: Option<Sender<BackgroundCommand>>,
    pub is_loading: bool,
    pub is_connected: bool,
    pub current_host: String,
    last_ssh_config: Option<SshConfig>,

    // Navigation
    pub scroll_offset: usize,
    pub selected_row: Option<usize>,

    // Mode
    pub mode: Mode,

    // Filter input
    pub filter_text: String,

    // Find
    pub find_text: String,
    pub find_regex: Option<regex::Regex>,  // cached, avoids recompile on every keypress
    pub find_matches: Vec<usize>,          // ordered row indices for navigation
    pub find_match_set: HashSet<usize>,    // same data, O(1) lookup in draw loop
    pub find_current: usize,

    // Incremental filter — how many entries have already been processed
    filtered_up_to: usize,

    // Connection form
    pub conn: ConnectForm,

    // Bookmarks view
    pub bookmark_offset: usize,
    pub bookmark_selected: usize,

    // Status
    pub status: String,

    // Settings
    pub save_settings: SaveSettings,

    // CLI
    pending_file: Option<String>,
    pending_ssh_profile: Option<String>,

    pub should_quit: bool,
}

impl App {
    pub fn new(pending_file: Option<String>, pending_ssh_profile: Option<String>) -> Self {
        Self {
            log_store: LogStore::new(),
            filter: FilterCriteria::default(),
            filtered_indices: Vec::new(),
            bookmarks: HashSet::new(),
            bg_receiver: None,
            bg_cmd_sender: None,
            is_loading: false,
            is_connected: false,
            current_host: String::new(),
            last_ssh_config: None,
            scroll_offset: 0,
            selected_row: None,
            mode: Mode::Normal,
            filter_text: String::new(),
            find_text: String::new(),
            find_regex: None,
            find_matches: Vec::new(),
            find_match_set: HashSet::new(),
            find_current: 0,
            filtered_up_to: 0,
            conn: ConnectForm::default(),
            bookmark_offset: 0,
            bookmark_selected: 0,
            status: "Press ? for help | c = connect SSH | o = open file".to_string(),
            save_settings: load_settings(),
            pending_file,
            pending_ssh_profile,
            should_quit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
        if let Some(path) = self.pending_file.take() {
            self.open_file(path);
        }
        if let Some(name) = self.pending_ssh_profile.take() {
            self.connect_profile(&name);
        }

        loop {
            self.process_messages();
            terminal.draw(|f| crate::ui::draw(f, self))?;

            if crossterm::event::poll(Duration::from_millis(50))? {
                match crossterm::event::read()? {
                    Event::Key(key) => self.handle_key(key),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            if self.should_quit {
                break;
            }
        }

        if self.save_settings.auto_save && !self.log_store.entries.is_empty() {
            self.save_now();
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Filter => self.handle_filter(key),
            Mode::Find   => self.handle_find(key),
            Mode::Connect => self.handle_connect(key),
            Mode::Bookmarks => self.handle_bookmarks(key),
            Mode::Help => { self.mode = Mode::Normal; }
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('c') => {
                self.conn = ConnectForm::default();
                self.mode = Mode::Connect;
            }
            KeyCode::Char('o') => {
                self.status = "Enter file path in filter bar and press Enter".to_string();
                // Repurpose filter bar for file path entry
                self.filter_text = String::new();
                self.mode = Mode::Filter; // Will detect if input looks like a path
            }
            KeyCode::Char('r') => {
                if let Some(config) = self.last_ssh_config.clone() {
                    self.start_ssh(config);
                }
            }
            KeyCode::Char('d') => self.disconnect(),
            KeyCode::Char('/') => {
                self.mode = Mode::Filter;
                self.status = "Filter (regex) — Enter to apply, Esc to clear".to_string();
            }
            KeyCode::Char('f') | KeyCode::Char('F') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = Mode::Find;
                self.status = "Find — n/N to navigate, Esc to close".to_string();
            }
            KeyCode::Char('n') => self.find_next(),
            KeyCode::Char('N') => self.find_prev(),
            KeyCode::Char('b') => self.toggle_bookmark(),
            KeyCode::Char('B') => {
                self.mode = Mode::Bookmarks;
                self.bookmark_selected = 0;
                self.bookmark_offset = 0;
            }
            KeyCode::Char('g') => self.go_to_top(),
            KeyCode::Char('G') => self.go_to_bottom(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(1),
            KeyCode::Up   | KeyCode::Char('k') => self.move_up(1),
            KeyCode::PageDown => self.move_down(20),
            KeyCode::PageUp   => self.move_up(20),
            KeyCode::Esc => {
                // Clear filter
                self.filter_text.clear();
                self.filter.set_pattern("");
                self.apply_filter();
                self.status = "Filter cleared".to_string();
            }
            _ => {}
        }
    }

    fn handle_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = String::new();
            }
            KeyCode::Enter => {
                let text = self.filter_text.trim().to_string();
                // If it looks like a file path, open it
                if text.starts_with('/') || text.starts_with('~') || text.starts_with('.') {
                    self.mode = Mode::Normal;
                    self.open_file(text);
                } else {
                    if !self.filter.set_pattern(&text) {
                        self.status = "Invalid regex".to_string();
                    } else {
                        self.apply_filter();
                        self.mode = Mode::Normal;
                        self.status = format!("Showing {} / {} entries", self.filtered_indices.len(), self.log_store.entries.len());
                    }
                }
            }
            KeyCode::Backspace => { self.filter_text.pop(); }
            KeyCode::Char(c) => self.filter_text.push(c),
            _ => {}
        }
    }

    fn handle_find(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.find_text.clear();
                self.find_regex = None;
                self.find_matches.clear();
                self.find_match_set.clear();
                self.status = String::new();
            }
            KeyCode::Enter | KeyCode::Down | KeyCode::Char('n') => self.find_next(),
            KeyCode::Up    | KeyCode::Char('N') => self.find_prev(),
            KeyCode::Backspace => {
                self.find_text.pop();
                self.recompile_find_regex();
            }
            KeyCode::Char(c) => {
                self.find_text.push(c);
                self.recompile_find_regex();
            }
            _ => {}
        }
    }

    fn handle_connect(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Tab => {
                self.conn.focused = (self.conn.focused + 1) % self.conn.field_count();
            }
            KeyCode::BackTab => {
                let n = self.conn.field_count();
                self.conn.focused = (self.conn.focused + n - 1) % n;
            }
            KeyCode::Enter => {
                let focused = self.conn.focused;
                match focused {
                    7 => { // Connect button
                        match self.conn.to_ssh_config() {
                            Ok(config) => {
                                self.mode = Mode::Normal;
                                self.start_ssh(config);
                            }
                            Err(e) => self.conn.error = e,
                        }
                    }
                    8 => { // Save profile
                        self.conn.save_current_profile();
                        self.conn.error = "Profile saved".to_string();
                    }
                    9 => self.mode = Mode::Normal, // Cancel
                    // Profile selection
                    6 => {
                        if let Some(idx) = self.conn.profile_idx {
                            let next = (idx + 1) % self.conn.profiles.len().max(1);
                            self.conn.load_profile(next);
                        } else if !self.conn.profiles.is_empty() {
                            self.conn.load_profile(0);
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Left | KeyCode::Right if self.conn.focused == 3 => {
                // Toggle auth choice
                self.conn.auth_choice = (self.conn.auth_choice + 1) % 3;
            }
            KeyCode::Up if self.conn.focused == 6 => {
                // Scroll profiles up
                if let Some(idx) = self.conn.profile_idx {
                    if idx > 0 { self.conn.load_profile(idx - 1); }
                } else if !self.conn.profiles.is_empty() {
                    self.conn.load_profile(0);
                }
            }
            KeyCode::Down if self.conn.focused == 6 => {
                // Scroll profiles down
                let n = self.conn.profiles.len();
                if n == 0 { return; }
                let next = self.conn.profile_idx.map(|i| (i + 1) % n).unwrap_or(0);
                self.conn.load_profile(next);
            }
            KeyCode::Backspace => {
                match self.conn.focused {
                    0 => { self.conn.host.pop(); }
                    1 => { self.conn.port.pop(); }
                    2 => { self.conn.username.pop(); }
                    4 => { if self.conn.auth_choice == 0 { self.conn.password.pop(); } else { self.conn.key_path.pop(); } }
                    5 => { self.conn.command.pop(); }
                    9 => { self.conn.profile_name.pop(); }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                match self.conn.focused {
                    0 => self.conn.host.push(c),
                    1 => self.conn.port.push(c),
                    2 => self.conn.username.push(c),
                    4 => { if self.conn.auth_choice == 0 { self.conn.password.push(c); } else { self.conn.key_path.push(c); } }
                    5 => self.conn.command.push(c),
                    9 => self.conn.profile_name.push(c),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn handle_bookmarks(&mut self, key: KeyEvent) {
        let sorted: Vec<usize> = {
            let mut v: Vec<usize> = self.bookmarks.iter().copied().collect();
            v.sort();
            v
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('B') | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !sorted.is_empty() {
                    self.bookmark_selected = (self.bookmark_selected + 1).min(sorted.len() - 1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.bookmark_selected = self.bookmark_selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(&entry_idx) = sorted.get(self.bookmark_selected) {
                    if let Some(row) = self.filtered_indices.iter().position(|&i| i == entry_idx) {
                        self.selected_row = Some(row);
                        self.scroll_to(row);
                        self.mode = Mode::Normal;
                    } else {
                        self.status = "Entry is hidden by active filter".to_string();
                    }
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(&entry_idx) = sorted.get(self.bookmark_selected) {
                    self.bookmarks.remove(&entry_idx);
                    self.bookmark_selected = self.bookmark_selected.saturating_sub(1);
                }
            }
            KeyCode::Char('c') => {
                self.bookmarks.clear();
                self.bookmark_selected = 0;
            }
            _ => {}
        }
    }

    // --- Log navigation ---

    pub fn move_down(&mut self, n: usize) {
        let len = self.filtered_indices.len();
        if len == 0 { return; }
        let cur = self.selected_row.unwrap_or(0);
        let next = (cur + n).min(len - 1);
        self.selected_row = Some(next);
        self.scroll_to(next);
    }

    pub fn move_up(&mut self, n: usize) {
        let cur = self.selected_row.unwrap_or(0);
        let next = cur.saturating_sub(n);
        self.selected_row = Some(next);
        self.scroll_to(next);
    }

    pub fn go_to_top(&mut self) {
        self.selected_row = if self.filtered_indices.is_empty() { None } else { Some(0) };
        self.scroll_offset = 0;
    }

    pub fn go_to_bottom(&mut self) {
        let len = self.filtered_indices.len();
        if len == 0 { self.selected_row = None; return; }
        self.selected_row = Some(len - 1);
        self.scroll_to(len - 1);
    }

    pub fn scroll_to(&mut self, row: usize) {
        // viewport height is unknown here; use a rough estimate, ui will correct it
        if row < self.scroll_offset {
            self.scroll_offset = row;
        } else if row >= self.scroll_offset + 40 {
            self.scroll_offset = row.saturating_sub(39);
        }
    }

    pub fn ensure_selected_visible(&mut self, visible_rows: usize) {
        if visible_rows == 0 { return; }
        if let Some(row) = self.selected_row {
            if row < self.scroll_offset {
                self.scroll_offset = row;
            } else if row >= self.scroll_offset + visible_rows {
                self.scroll_offset = row + 1 - visible_rows;
            }
        }
    }

    // --- Bookmarks ---

    pub fn toggle_bookmark(&mut self) {
        if let Some(row) = self.selected_row {
            if let Some(&entry_idx) = self.filtered_indices.get(row) {
                if self.bookmarks.contains(&entry_idx) {
                    self.bookmarks.remove(&entry_idx);
                    self.status = format!("Bookmark removed (line {})", entry_idx + 1);
                } else {
                    self.bookmarks.insert(entry_idx);
                    self.status = format!("Bookmarked line {} — {} total", entry_idx + 1, self.bookmarks.len());
                }
            }
        }
    }

    // --- Find ---

    /// Recompile the find regex when search text changes.
    pub fn recompile_find_regex(&mut self) {
        if self.find_text.is_empty() {
            self.find_regex = None;
        } else {
            self.find_regex = regex::RegexBuilder::new(&regex::escape(&self.find_text))
                .case_insensitive(true)
                .build()
                .ok();
        }
        self.update_find_matches();
    }

    /// Rebuild match list using the already-compiled regex.
    pub fn update_find_matches(&mut self) {
        self.find_matches.clear();
        self.find_match_set.clear();
        self.find_current = 0;
        let re = match &self.find_regex {
            Some(r) => r.clone(),
            None => return,
        };
        for (row_idx, &entry_idx) in self.filtered_indices.iter().enumerate() {
            if let Some(entry) = self.log_store.entries.get(entry_idx) {
                if re.is_match(&entry.message) {
                    self.find_matches.push(row_idx);
                    self.find_match_set.insert(row_idx);
                }
            }
        }
        self.status = format!("{} match(es) for '{}'", self.find_matches.len(), self.find_text);
    }

    pub fn find_next(&mut self) {
        if self.find_matches.is_empty() { return; }
        self.find_current = (self.find_current + 1) % self.find_matches.len();
        let row = self.find_matches[self.find_current];
        self.selected_row = Some(row);
        self.scroll_to(row);
    }

    pub fn find_prev(&mut self) {
        if self.find_matches.is_empty() { return; }
        let n = self.find_matches.len();
        self.find_current = (self.find_current + n - 1) % n;
        let row = self.find_matches[self.find_current];
        self.selected_row = Some(row);
        self.scroll_to(row);
    }

    // --- Filter ---

    /// Full rebuild — call only when filter criteria changes.
    pub fn apply_filter(&mut self) {
        self.filtered_indices.clear();
        self.filtered_up_to = 0;
        self.extend_filter();
        self.selected_row = self.selected_row.map(|r| r.min(self.filtered_indices.len().saturating_sub(1)));
        if !self.find_text.is_empty() {
            self.update_find_matches();
        }
    }

    /// Incremental — only checks entries added since last call.
    fn extend_filter(&mut self) {
        let pin = self.save_settings.always_show_bookmarks;
        let total = self.log_store.entries.len();
        for i in self.filtered_up_to..total {
            let entry = &self.log_store.entries[i];
            if self.filter.matches(entry) || (pin && self.bookmarks.contains(&i)) {
                self.filtered_indices.push(i);
            }
        }
        self.filtered_up_to = total;
    }

    // --- Connection ---

    pub fn connect_profile(&mut self, name: &str) {
        match config_for_profile(name) {
            Some(config) => self.start_ssh(config),
            None => self.status = format!("Profile '{}' not found", name),
        }
    }

    pub fn start_ssh(&mut self, config: SshConfig) {
        self.reset_state();
        self.current_host = config.host.clone();
        self.last_ssh_config = Some(config.clone());
        self.is_loading = true;
        self.status = format!("Connecting to {}...", config.host);

        let (tx, rx) = crossbeam_channel::unbounded();
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        self.bg_receiver = Some(rx);
        self.bg_cmd_sender = Some(cmd_tx);
        ssh_reader::start_ssh(config, tx, cmd_rx);
    }

    pub fn open_file(&mut self, path: String) {
        self.reset_state();
        self.current_host = path.clone();
        self.is_loading = true;
        self.status = format!("Loading {}...", path);

        let (tx, rx) = crossbeam_channel::unbounded();
        self.bg_receiver = Some(rx);
        file_reader::read_file(path, tx);
    }

    pub fn disconnect(&mut self) {
        if let Some(ref sender) = self.bg_cmd_sender {
            let _ = sender.send(BackgroundCommand::Disconnect);
        }
        self.bg_cmd_sender = None;
        self.is_connected = false;
        self.is_loading = false;
        self.status = "Disconnected".to_string();
    }

    fn reset_state(&mut self) {
        if let Some(ref sender) = self.bg_cmd_sender {
            let _ = sender.send(BackgroundCommand::Cancel);
        }
        self.log_store = LogStore::new();
        self.filtered_indices.clear();
        self.bg_receiver = None;
        self.bg_cmd_sender = None;
        self.is_loading = false;
        self.is_connected = false;
        self.scroll_offset = 0;
        self.selected_row = None;
        self.filter_text.clear();
        self.filter = FilterCriteria::default();
        self.find_text.clear();
        self.find_regex = None;
        self.find_matches.clear();
        self.find_match_set.clear();
        self.filtered_up_to = 0;
        self.bookmarks.clear();
    }

    // --- Background message processing ---

    pub fn process_messages(&mut self) {
        let receiver = match &self.bg_receiver {
            Some(rx) => rx.clone(),
            None => return,
        };

        let mut count = 0;
        loop {
            match receiver.try_recv() {
                Ok(msg) => {
                    self.handle_message(msg);
                    count += 1;
                    if count >= 5000 { break; }
                }
                Err(_) => break,
            }
        }

        if count > 0 {
            self.extend_filter();
            // Auto-scroll to bottom while loading
            if self.is_loading && self.selected_row.is_none() {
                let len = self.filtered_indices.len();
                if len > 0 {
                    self.scroll_offset = len.saturating_sub(40);
                }
            }
        }
    }

    fn handle_message(&mut self, msg: BackgroundMessage) {
        match msg {
            BackgroundMessage::Entry(entry) => {
                self.log_store.services.insert(entry.service.clone());
                self.log_store.entries.push(entry);
            }
            BackgroundMessage::Progress { lines, .. } => {
                self.status = format!("Loading... {} lines", lines);
            }
            BackgroundMessage::Completed { total_lines, entries } => {
                self.is_loading = false;
                self.status = format!("Loaded {} entries from {} lines — {}", entries, total_lines, self.current_host);
            }
            BackgroundMessage::Error(e) => {
                self.is_loading = false;
                self.status = format!("Error: {}", e);
            }
            BackgroundMessage::SshConnected => {
                self.is_connected = true;
                self.is_loading = false;
                self.status = format!("Connected to {} — streaming live", self.current_host);
            }
            BackgroundMessage::SshDisconnected => {
                self.is_connected = false;
                self.status = format!("Disconnected from {}", self.current_host);
                if self.save_settings.auto_save && !self.log_store.entries.is_empty() {
                    self.save_now();
                }
            }
        }
    }

    // --- Save ---

    pub fn save_now(&mut self) {
        let path = self.save_settings.resolve_filename(&self.current_host);
        let entries: Vec<&LogEntry> = if self.save_settings.save_filtered_only {
            self.filtered_indices.iter()
                .filter_map(|&i| self.log_store.entries.get(i))
                .collect()
        } else {
            self.log_store.entries.iter().collect()
        };

        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let lines: Vec<String> = entries.iter()
            .map(|e| format!("{} {} [{}] {}", e.timestamp, e.service, e.priority, e.message))
            .collect();
        let _ = std::fs::write(&path, lines.join("\n"));
        self.status = format!("Saved {} entries to {}", entries.len(), path);
    }
}
