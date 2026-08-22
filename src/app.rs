use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use regex::Regex;
use std::collections::HashSet;

use crate::analyzer::{LogStore, FilterCriteria};
use crate::background::{BackgroundMessage, BackgroundCommand};
use crate::ui::connection_dialog::ConnectionDialog;
use crate::ui::filter_bar::FilterBar;
use crate::ui::log_viewer::{LogViewer, priority_label, priority_color};
use crate::ui::open_file_dialog::OpenFileDialog;
use crate::ui::save_settings::{SaveFormat, SaveSettings, SaveSettingsDialog, load_settings, save_settings_to_disk};
use crate::ui::saved_patterns::{SavedPatterns, load_patterns};
use crate::ui::testcase::{TestCase, TestCaseDialog};
use crate::workers::{file_reader, log_writer, ssh_reader};

fn read_memory_kb() -> u64 {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Pss:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/app_tests.rs"]
mod tests;

struct FindState {
    active: bool,
    search_text: String,
    regex: Option<Regex>,
    /// Row indices into `filtered_indices` that match
    match_indices: Vec<usize>,
    /// Current match position within match_indices
    current_match: usize,
    /// Whether the find input should request focus this frame
    request_focus: bool,
}

pub struct JlogApp {
    log_store: LogStore,
    filter: FilterCriteria,
    filtered_indices: Vec<usize>,
    filtered_up_to: usize,

    open_file_dialog: OpenFileDialog,
    connection_dialog: ConnectionDialog,
    filter_bar: FilterBar,
    log_viewer: LogViewer,

    bg_receiver: Option<Receiver<BackgroundMessage>>,
    bg_cmd_sender: Option<Sender<BackgroundCommand>>,

    is_loading: bool,
    is_connected: bool,
    status_message: String,
    total_lines: usize,
    cached_services: Vec<String>,

    save_settings: SaveSettings,
    save_settings_dialog: SaveSettingsDialog,
    current_host: String,

    /// Last SSH config used (for reconnect)
    last_ssh_config: Option<ssh_reader::SshConfig>,

    /// File to load on first frame (from CLI argument)
    pending_file: Option<String>,

    /// SSH profile name to connect on first frame (from CLI --profile/-p argument)
    pending_ssh_profile: Option<String>,

    find: FindState,

    show_help: bool,

    /// Saved filter bar state for "Show in Context" feature
    saved_filter_bar: Option<FilterBar>,

    /// Bookmarked entry indices (for timeline view)
    bookmarks: HashSet<usize>,
    /// Whether the bookmark/timeline window is open
    show_bookmarks: bool,

    /// Named regexes saved for reuse (persisted to disk).
    saved_patterns: SavedPatterns,

    /// Test cases recorded this session.
    test_cases: Vec<TestCase>,
    /// Index into `test_cases` of the one currently recording, if any.
    active_test_case: Option<usize>,
    test_case_dialog: TestCaseDialog,
    show_test_cases: bool,

    memory_kb: u64,
    last_memory_check: std::time::Instant,
}

impl JlogApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Parse CLI arguments: jlog [<file>] [--profile|-p <name>]
        let mut args = std::env::args().skip(1);
        let mut pending_file = None;
        let mut pending_ssh_profile = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("Usage: jlog [OPTIONS]");
                    println!();
                    println!("Options:");
                    println!("  -f, --file <PATH>       Open a log file directly");
                    println!("  -p, --profile <NAME>    Connect using a saved SSH profile");
                    println!("  -h, --help              Print this help message");
                    std::process::exit(0);
                }
                "-f" | "--file" => {
                    pending_file = args.next();
                }
                "-p" | "--profile" => {
                    pending_ssh_profile = args.next();
                }
                _ => {}
            }
        }

        Self {
            log_store: LogStore::new(),
            filter: FilterCriteria::default(),
            filtered_indices: Vec::new(),
            filtered_up_to: 0,

            open_file_dialog: OpenFileDialog::default(),
            connection_dialog: ConnectionDialog::default(),
            filter_bar: FilterBar::default(),
            log_viewer: LogViewer::default(),

            bg_receiver: None,
            bg_cmd_sender: None,

            is_loading: false,
            is_connected: false,
            status_message: "Ready - File > Open or Connect SSH".to_string(),
            total_lines: 0,
            cached_services: Vec::new(),

            save_settings: load_settings(),
            save_settings_dialog: SaveSettingsDialog::default(),
            current_host: "local".to_string(),

            last_ssh_config: None,

            pending_file,

            pending_ssh_profile,

            find: FindState {
                active: false,
                search_text: String::new(),
                regex: None,
                match_indices: Vec::new(),
                current_match: 0,
                request_focus: false,
            },

            show_help: false,

            saved_filter_bar: None,

            bookmarks: HashSet::new(),
            show_bookmarks: false,

            saved_patterns: load_patterns(),

            test_cases: Vec::new(),
            active_test_case: None,
            test_case_dialog: TestCaseDialog::default(),
            show_test_cases: false,

            memory_kb: read_memory_kb(),
            last_memory_check: std::time::Instant::now(),
        }
    }

    fn load_file(&mut self, path: String) {
        self.reset_state();
        self.current_host = "local".to_string();
        self.is_loading = true;
        self.status_message = format!("Loading: {}", path);

        let (tx, rx) = unbounded();
        self.bg_receiver = Some(rx);
        file_reader::read_file(path, tx);
    }

    fn start_ssh(&mut self, config: ssh_reader::SshConfig) {
        self.reset_state();
        self.current_host = config.host.clone();
        self.last_ssh_config = Some(config.clone());
        self.is_loading = true;
        self.is_connected = false;
        self.status_message = format!("Connecting to {}...", config.host);

        let (tx, rx) = unbounded();
        let (cmd_tx, cmd_rx) = unbounded();
        self.bg_receiver = Some(rx);
        self.bg_cmd_sender = Some(cmd_tx);
        ssh_reader::start_ssh(config, tx, cmd_rx);
    }

    fn disconnect(&mut self) {
        if let Some(ref sender) = self.bg_cmd_sender {
            let _ = sender.send(BackgroundCommand::Disconnect);
        }
        self.bg_cmd_sender = None;
        self.is_connected = false;
        self.is_loading = false;
        self.status_message = "Disconnected".to_string();
    }

    fn reset_state(&mut self) {
        if let Some(ref sender) = self.bg_cmd_sender {
            let _ = sender.send(BackgroundCommand::Cancel);
        }
        self.log_store = LogStore::new();
        self.filtered_indices.clear();
        self.filtered_up_to = 0;
        self.bg_receiver = None;
        self.bg_cmd_sender = None;
        self.is_loading = false;
        self.is_connected = false;
        self.total_lines = 0;
        self.cached_services.clear();
        self.filter_bar = FilterBar::default();
        self.filter = FilterCriteria::default();
        self.bookmarks.clear();
        self.show_bookmarks = false;
        self.test_cases.clear();
        self.active_test_case = None;
        self.show_test_cases = false;
    }

    fn process_messages(&mut self) {
        let receiver = match &self.bg_receiver {
            Some(rx) => rx.clone(),
            None => return,
        };

        let mut new_entries = false;
        let entries_before = self.log_store.entries.len();
        // Drain up to 5000 messages per frame to stay responsive
        for _ in 0..5000 {
            match receiver.try_recv() {
                Ok(msg) => match msg {
                    BackgroundMessage::Entry(entry) => {
                        self.log_store.services.insert(entry.service.clone());
                        self.log_store.entries.push(entry);
                        new_entries = true;
                    }
                    BackgroundMessage::Progress { lines, percent } => {
                        self.total_lines = lines;
                        if percent > 0.0 {
                            self.status_message = format!(
                                "Loading: {} lines ({:.1}%) - {} entries",
                                lines, percent, self.log_store.entries.len()
                            );
                        } else {
                            self.status_message = format!(
                                "Streaming: {} lines - {} entries",
                                lines, self.log_store.entries.len()
                            );
                        }
                    }
                    BackgroundMessage::Completed { total_lines, entries } => {
                        self.is_loading = false;
                        self.total_lines = total_lines;
                        self.status_message = format!(
                            "Loaded {} entries from {} lines",
                            entries, total_lines
                        );
                    }
                    BackgroundMessage::Error(e) => {
                        self.is_loading = false;
                        self.status_message = format!("Error: {}", e);
                    }
                    BackgroundMessage::SshConnected => {
                        self.is_connected = true;
                        self.status_message = "SSH connected - streaming...".to_string();
                    }
                    BackgroundMessage::SshDisconnected => {
                        self.is_connected = false;
                        self.is_loading = false;
                        if self.save_settings.auto_save && !self.log_store.entries.is_empty() {
                            self.save_now();
                        }
                        if !self.status_message.starts_with("Error") && !self.status_message.starts_with("Saved") {
                            self.status_message = format!(
                                "Disconnected - {} entries loaded",
                                self.log_store.entries.len()
                            );
                        }
                    }
                },
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.bg_receiver = None;
                    break;
                }
            }
        }

        if new_entries {
            let new_count = self.log_store.entries.len() - entries_before;
            self.log_viewer.notify_new_entries(new_count);
            self.extend_filter();
            if self.log_store.services.len() != self.cached_services.len() {
                self.cached_services = self.log_store.service_names();
            }
        }
    }

    fn handle_resize_edges(&self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if maximized { return; }

        let rect = ctx.screen_rect();
        let grip = 8.0;

        // Classify a screen point into an optional resize direction + the
        // cursor that should be shown for it.
        let classify = |pos: egui::Pos2| {
            let north = pos.y <= rect.min.y + grip;
            let south = pos.y >= rect.max.y - grip;
            let west  = pos.x <= rect.min.x + grip;
            let east  = pos.x >= rect.max.x - grip;

            use egui::viewport::ResizeDirection::*;
            match (north, south, west, east) {
                (true,  _,     true,  _    ) => Some((NorthWest, egui::CursorIcon::ResizeNwSe)),
                (true,  _,     _,     true ) => Some((NorthEast, egui::CursorIcon::ResizeNeSw)),
                (_,     true,  true,  _    ) => Some((SouthWest, egui::CursorIcon::ResizeNeSw)),
                (_,     true,  _,     true ) => Some((SouthEast, egui::CursorIcon::ResizeNwSe)),
                (true,  _,     _,     _    ) => Some((North,     egui::CursorIcon::ResizeNorth)),
                (_,     true,  _,     _    ) => Some((South,     egui::CursorIcon::ResizeSouth)),
                (_,     _,     true,  _    ) => Some((West,      egui::CursorIcon::ResizeWest)),
                (_,     _,     _,     true ) => Some((East,      egui::CursorIcon::ResizeEast)),
                _                            => None,
            }
        };

        let (hover_pos, pressed, press_origin) = ctx.input(|i| (
            i.pointer.hover_pos(),
            i.pointer.primary_pressed(),
            i.pointer.press_origin(),
        ));

        // Show the resize cursor only while hovering an actual edge.
        if let Some((_, cursor)) = hover_pos.and_then(classify) {
            ctx.set_cursor_icon(cursor);
        }

        // Begin a resize only when the press *originated* on an edge. Checking
        // the press origin (rather than the live pointer) prevents a window
        // move started on the title bar from turning into a resize when the
        // pointer crosses the top edge mid-drag.
        if pressed {
            if let Some((direction, _)) = press_origin.and_then(classify) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }

    fn show_title_bar(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Reserve the top grip (matching handle_resize_edges) for the window
        // resize handle so dragging the bar to move the window can't be hijacked
        // by the top-edge resize zone.
        let grip = 8.0;
        let mut drag_rect = ui.max_rect();
        drag_rect.min.y += grip;
        let drag_response = ui.interact(
            drag_rect,
            ui.id().with("title_bar_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag_response.double_clicked() {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }

        let dark = egui::Color32::from_rgb(30, 30, 30);

        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("jlog").strong().color(dark));
            if self.current_host != "local" && !self.current_host.is_empty() {
                ui.label(egui::RichText::new(format!("— {}", self.current_host)).color(dark));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);

                let close_btn = ui.add(egui::Button::new(egui::RichText::new("✕").color(egui::Color32::from_rgb(196, 43, 28))).frame(false).min_size(egui::vec2(24.0, 24.0)));
                if close_btn.clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                let max_icon = if maximized { "❐" } else { "▢" };
                if ui.add(egui::Button::new(egui::RichText::new(max_icon).color(dark)).frame(false).min_size(egui::vec2(24.0, 24.0))).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }

                if ui.add(egui::Button::new(egui::RichText::new("−").color(dark)).frame(false).min_size(egui::vec2(24.0, 24.0))).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            });
        });
    }

    fn export_filtered(&mut self) {
        if self.filtered_indices.is_empty() {
            self.status_message = "Nothing to export — no entries match the current filter".to_string();
            return;
        }

        let filename = format!(
            "jlog_export_{}.log",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let path = std::path::Path::new(&self.save_settings.destination).join(&filename);

        let entries: Vec<&crate::analyzer::LogEntry> = self.filtered_indices
            .iter()
            .map(|&i| &self.log_store.entries[i])
            .collect();

        match log_writer::write_entries(&path, &entries, &SaveFormat::PlainText) {
            Ok(()) => self.status_message = format!(
                "Exported {} entries to {}",
                entries.len(),
                path.display()
            ),
            Err(e) => self.status_message = format!("Export error: {}", e),
        }
    }

    /// Replace characters unsafe for a filename with underscores.
    fn sanitize_filename(s: &str) -> String {
        let cleaned: String = s
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        if cleaned.is_empty() { "unnamed".to_string() } else { cleaned }
    }

    fn format_ext(&self) -> &'static str {
        match self.save_settings.format {
            SaveFormat::Json => "json",
            SaveFormat::PlainText => "log",
        }
    }

    /// Write a single test case's captured entries to its own file. Returns the
    /// path written on success.
    fn export_test_case(&self, tc: &TestCase) -> anyhow::Result<String> {
        let range = tc.range(self.log_store.entries.len());
        let entries: Vec<&crate::analyzer::LogEntry> =
            self.log_store.entries[range].iter().collect();

        let filename = format!(
            "testcase_{}_{}.{}",
            Self::sanitize_filename(&tc.id),
            chrono::Local::now().format("%Y%m%d_%H%M%S"),
            self.format_ext(),
        );
        let path = std::path::Path::new(&self.save_settings.destination).join(filename);
        log_writer::write_entries(&path, &entries, &self.save_settings.format)?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Write the full session ("mother") log capturing every entry.
    fn export_session_log(&self) -> anyhow::Result<String> {
        let entries: Vec<&crate::analyzer::LogEntry> = self.log_store.entries.iter().collect();
        let filename = format!(
            "session_{}_{}.{}",
            Self::sanitize_filename(&self.current_host),
            chrono::Local::now().format("%Y%m%d_%H%M%S"),
            self.format_ext(),
        );
        let path = std::path::Path::new(&self.save_settings.destination).join(filename);
        log_writer::write_entries(&path, &entries, &self.save_settings.format)?;
        Ok(path.to_string_lossy().to_string())
    }

    /// Export every test case to a standalone file, plus the mother log when
    /// more than one test case ran this session.
    fn export_all_test_cases(&mut self) {
        if self.test_cases.is_empty() {
            self.status_message = "No test cases to export".to_string();
            return;
        }

        let cases = self.test_cases.clone();
        let mut written = 0usize;
        let mut errors: Vec<String> = Vec::new();

        for tc in &cases {
            match self.export_test_case(tc) {
                Ok(_) => written += 1,
                Err(e) => errors.push(format!("{}: {}", tc.id, e)),
            }
        }

        let mut mother = false;
        if cases.len() > 1 {
            match self.export_session_log() {
                Ok(_) => mother = true,
                Err(e) => errors.push(format!("session log: {}", e)),
            }
        }

        if errors.is_empty() {
            self.status_message = format!(
                "Exported {} test case file(s){} to {}",
                written,
                if mother { " + session log" } else { "" },
                self.save_settings.destination,
            );
        } else {
            self.status_message = format!(
                "Exported {} file(s), {} error(s): {}",
                written,
                errors.len(),
                errors.join("; "),
            );
        }
    }


    fn start_test_case(&mut self, id: String, name: String, description: String) {
        let start_entry = self.log_store.entries.len();
        self.test_cases.push(TestCase {
            id: id.clone(),
            name,
            description,
            start_entry,
            end_entry: None,
        });
        self.active_test_case = Some(self.test_cases.len() - 1);
        self.status_message = format!("Recording test case '{}' from entry {}", id, start_entry);
    }

    fn stop_test_case(&mut self) {
        if let Some(idx) = self.active_test_case.take() {
            let end = self.log_store.entries.len();
            if let Some(tc) = self.test_cases.get_mut(idx) {
                tc.end_entry = Some(end);
                let captured = end.saturating_sub(tc.start_entry);
                self.status_message = format!(
                    "Stopped test case '{}' — captured {} entries",
                    tc.id, captured
                );
            }
        }
    }

    fn save_now(&mut self) {
        let entries: Vec<&crate::analyzer::LogEntry> = if self.save_settings.save_filtered_only {
            self.filtered_indices
                .iter()
                .map(|&i| &self.log_store.entries[i])
                .collect()
        } else {
            self.log_store.entries.iter().collect()
        };

        if entries.is_empty() {
            self.status_message = "Nothing to save - no log entries".to_string();
            return;
        }

        match log_writer::save_logs(&entries, &self.save_settings, &self.current_host) {
            Ok(path) => {
                self.status_message = format!("Saved {} entries to {}", entries.len(), path);
            }
            Err(e) => {
                self.status_message = format!("Save error: {}", e);
            }
        }
    }

    fn update_find_matches(&mut self) {
        self.find.match_indices.clear();
        self.find.current_match = 0;
        if let Some(ref re) = self.find.regex {
            for (row_idx, &entry_idx) in self.filtered_indices.iter().enumerate() {
                if let Some(entry) = self.log_store.entries.get(entry_idx) {
                    if re.is_match(&entry.message) {
                        self.find.match_indices.push(row_idx);
                    }
                }
            }
        }
    }

    fn find_jump_to_current(&mut self) {
        if let Some(&row) = self.find.match_indices.get(self.find.current_match) {
            self.log_viewer.scroll_to_row = Some(row);
        }
    }

    fn apply_filter(&mut self) {
        self.filtered_indices.clear();
        self.filtered_up_to = 0;
        self.extend_filter();
        if self.find.active {
            self.update_find_matches();
        }
    }

    /// Only checks entries added since the last filter pass.
    fn extend_filter(&mut self) {
        let pin_bookmarks = self.save_settings.always_show_bookmarks;
        let total = self.log_store.entries.len();
        for i in self.filtered_up_to..total {
            let entry = &self.log_store.entries[i];
            if self.filter.matches(entry) || (pin_bookmarks && self.bookmarks.contains(&i)) {
                self.filtered_indices.push(i);
            }
        }
        self.filtered_up_to = total;
    }
}

impl eframe::App for JlogApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.save_settings.auto_save && self.current_host != "local" && !self.log_store.entries.is_empty() {
            self.save_now();
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // Use exact egui panel background to prevent artifacts on resize/maximize
        let c = visuals.panel_fill;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background worker at 50 ms when active; low-frequency heartbeat
        // when idle to handle WSL2/X11 resize artifacts without burning full 60 fps.
        if self.bg_receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        self.handle_resize_edges(ctx);

        let now = std::time::Instant::now();
        if now.duration_since(self.last_memory_check).as_secs() >= 1 {
            self.memory_kb = read_memory_kb();
            self.last_memory_check = now;
        }

        // Load file from CLI argument on first frame
        if let Some(path) = self.pending_file.take() {
            self.load_file(path);
        }

        // Connect to SSH profile from CLI argument on first frame
        if let Some(name) = self.pending_ssh_profile.take() {
            if let Some(config) = crate::ui::connection_dialog::config_for_profile(&name) {
                self.start_ssh(config);
            } else {
                self.status_message = format!("SSH profile '{}' not found", name);
            }
        }

        // Poll background messages
        self.process_messages();

        // Dialogs
        if let Some(path) = self.open_file_dialog.show(ctx) {
            self.load_file(path);
        }
        if let Some(config) = self.connection_dialog.show(ctx) {
            self.start_ssh(config);
        }
        if let Some(new_settings) = self.save_settings_dialog.show(ctx) {
            save_settings_to_disk(&new_settings);
            self.save_settings = new_settings;
        }
        if let Some((id, name, description)) = self.test_case_dialog.show(ctx) {
            self.start_test_case(id, name, description);
        }

        // Help window
        if self.show_help {
            let mut open = self.show_help;
            egui::Window::new("Shortcuts & About")
                .open(&mut open)
                .resizable(true)
                .collapsible(false)
                .default_width(480.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Keyboard Shortcuts");
                    egui::Grid::new("shortcuts_grid")
                        .striped(true)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong("Shortcut");
                            ui.strong("Action");
                            ui.end_row();

                            ui.monospace("Ctrl+F");
                            ui.label("Find in logs");
                            ui.end_row();

                            ui.monospace("Escape");
                            ui.label("Close find bar");
                            ui.end_row();

                            ui.monospace("Enter / F3");
                            ui.label("Next find match");
                            ui.end_row();

                            ui.monospace("Shift+Enter / Shift+F3");
                            ui.label("Previous find match");
                            ui.end_row();

                            ui.monospace("Ctrl+C");
                            ui.label("Copy selected log line");
                            ui.end_row();

                            ui.monospace("B");
                            ui.label("Bookmark / unbookmark selected entry");
                            ui.end_row();

                            ui.monospace("Ctrl+B");
                            ui.label("Toggle bookmark timeline window");
                            ui.end_row();

                            ui.monospace("Right-click");
                            ui.label("Copy / bookmark menu");
                            ui.end_row();
                        });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.heading("Regex Filter Reference");
                    ui.add_space(4.0);
                    ui.label("The filter fields accept full Rust regex syntax.");
                    ui.add_space(6.0);

                    egui::Grid::new("regex_grid")
                        .striped(true)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong("Pattern");
                            ui.strong("Matches");
                            ui.end_row();

                            ui.monospace("error");
                            ui.label("Lines containing \"error\"");
                            ui.end_row();

                            ui.monospace("error|warning");
                            ui.label("Lines containing \"error\" OR \"warning\"");
                            ui.end_row();

                            ui.monospace("foo.*bar");
                            ui.label("\"foo\" followed by anything then \"bar\"");
                            ui.end_row();

                            ui.monospace("^kernel:");
                            ui.label("Lines starting with \"kernel:\"");
                            ui.end_row();

                            ui.monospace("timeout$");
                            ui.label("Lines ending with \"timeout\"");
                            ui.end_row();

                            ui.monospace("\\d+");
                            ui.label("One or more digits");
                            ui.end_row();

                            ui.monospace("[Ee]rror");
                            ui.label("\"Error\" or \"error\"");
                            ui.end_row();

                            ui.monospace("(?i)error");
                            ui.label("Case-insensitive match for \"error\"");
                            ui.end_row();

                            ui.monospace("failed (to|with)");
                            ui.label("\"failed to\" or \"failed with\"");
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    ui.label("Use the AND / OR / NOT combine mode to apply a second pattern.");
                    ui.label("NOT mode hides lines that match the pattern.");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.heading("About");
                    ui.label(format!("jlog v{}", env!("CARGO_PKG_VERSION")));
                    ui.label("A JSON log viewer and analyzer.");

                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        self.show_help = false;
                    }
                    }); // ScrollArea
                });
            self.show_help = open;
        }

        // Custom title bar (replaces OS decorations)
        egui::TopBottomPanel::top("title_bar")
            .exact_height(28.0)
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(220, 220, 220)))
            .show(ctx, |ui| {
                self.show_title_bar(ui, ctx);
            });

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open File...").clicked() {
                        ui.close_menu();
                        self.open_file_dialog.open = true;
                    }
                    ui.separator();
                    if ui.button("Save Logs Now").clicked() {
                        ui.close_menu();
                        self.save_now();
                    }
                    let export_label = format!(
                        "Export Filtered Logs ({})...",
                        self.filtered_indices.len()
                    );
                    if ui.button(export_label).clicked() {
                        ui.close_menu();
                        self.export_filtered();
                    }
                    if ui.button("Save Settings...").clicked() {
                        ui.close_menu();
                        self.save_settings_dialog.load_from(&self.save_settings);
                        self.save_settings_dialog.open = true;
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("SSH", |ui| {
                    if ui.button("Connect SSH...").clicked() {
                        ui.close_menu();
                        self.connection_dialog.open = true;
                    }

                    let profiles = self.connection_dialog.profiles().to_vec();
                    if !profiles.is_empty() {
                        ui.separator();
                        for (i, profile) in profiles.iter().enumerate() {
                            if ui.button(&profile.name).clicked() {
                                ui.close_menu();
                                self.connection_dialog.select_profile(i);
                            }
                        }
                    }

                    if self.is_connected {
                        ui.separator();
                        if ui.button("Disconnect").clicked() {
                            ui.close_menu();
                            self.disconnect();
                        }
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.log_viewer.auto_scroll, "Auto-scroll");
                    ui.separator();
                    let bookmark_label = format!("Bookmarks ({})...", self.bookmarks.len());
                    if ui.button(bookmark_label).clicked() {
                        self.show_bookmarks = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Test Case", |ui| {
                    let recording = self.active_test_case.is_some();
                    if ui.add_enabled(!recording, egui::Button::new("Start Test Case...")).clicked() {
                        ui.close_menu();
                        self.test_case_dialog.open();
                    }
                    if ui.add_enabled(recording, egui::Button::new("Stop Test Case")).clicked() {
                        ui.close_menu();
                        self.stop_test_case();
                    }
                    ui.separator();
                    let tc_label = format!("Test Cases ({})...", self.test_cases.len());
                    if ui.button(tc_label).clicked() {
                        ui.close_menu();
                        self.show_test_cases = true;
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Shortcuts & About...").clicked() {
                        self.show_help = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // Filter bar panel
        egui::TopBottomPanel::top("filter_bar").show(ctx, |ui| {
            if self.filter_bar.show(ui, &self.cached_services, &mut self.filter, &mut self.saved_patterns) {
                self.apply_filter();
            }
        });

        // Keyboard shortcut: Ctrl+F to open find bar
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
            self.find.active = true;
            self.find.request_focus = true;
        }

        // Keyboard shortcut: Ctrl+B to toggle bookmark window
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::B)) {
            self.show_bookmarks = !self.show_bookmarks;
        }

        // Handle bookmark toggle request from log viewer
        if let Some(entry_idx) = self.log_viewer.toggle_bookmark_requested.take() {
            if self.bookmarks.contains(&entry_idx) {
                self.bookmarks.remove(&entry_idx);
            } else {
                self.bookmarks.insert(entry_idx);
            }
        }

        // Find bar shortcuts (only when active)
        let mut find_next = false;
        let mut find_prev = false;
        let mut find_close = false;

        if self.find.active {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    find_close = true;
                }
                if i.key_pressed(egui::Key::F3) {
                    if i.modifiers.shift {
                        find_prev = true;
                    } else {
                        find_next = true;
                    }
                }
                if i.key_pressed(egui::Key::Enter) {
                    if i.modifiers.shift {
                        find_prev = true;
                    } else {
                        find_next = true;
                    }
                }
            });
        }

        if find_close {
            self.find.active = false;
            self.find.search_text.clear();
            self.find.regex = None;
            self.find.match_indices.clear();
            self.find.current_match = 0;
        }

        if find_next && !self.find.match_indices.is_empty() {
            self.find.current_match = (self.find.current_match + 1) % self.find.match_indices.len();
            self.find_jump_to_current();
        }
        if find_prev && !self.find.match_indices.is_empty() {
            if self.find.current_match == 0 {
                self.find.current_match = self.find.match_indices.len() - 1;
            } else {
                self.find.current_match -= 1;
            }
            self.find_jump_to_current();
        }

        // Find bar panel
        if self.find.active {
            egui::TopBottomPanel::top("find_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.find.search_text)
                            .desired_width(250.0)
                            .hint_text("Search in messages...")
                    );

                    if self.find.request_focus {
                        response.request_focus();
                        self.find.request_focus = false;
                    }

                    if response.changed() {
                        // Recompile regex on text change
                        if self.find.search_text.is_empty() {
                            self.find.regex = None;
                            self.find.match_indices.clear();
                            self.find.current_match = 0;
                        } else {
                            match regex::RegexBuilder::new(&regex::escape(&self.find.search_text))
                                .case_insensitive(true)
                                .build()
                            {
                                Ok(re) => {
                                    self.find.regex = Some(re);
                                    self.update_find_matches();
                                    // Jump to first match
                                    if !self.find.match_indices.is_empty() {
                                        self.find.current_match = 0;
                                        self.find_jump_to_current();
                                    }
                                }
                                Err(_) => {
                                    self.find.regex = None;
                                    self.find.match_indices.clear();
                                }
                            }
                        }
                    }

                    // Match counter
                    if self.find.search_text.is_empty() {
                        ui.label("0/0");
                    } else if self.find.match_indices.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "0/0");
                    } else {
                        ui.label(format!(
                            "{}/{}",
                            self.find.current_match + 1,
                            self.find.match_indices.len()
                        ));
                    }

                    if ui.small_button("\u{25B2} Prev").clicked() && !self.find.match_indices.is_empty() {
                        if self.find.current_match == 0 {
                            self.find.current_match = self.find.match_indices.len() - 1;
                        } else {
                            self.find.current_match -= 1;
                        }
                        self.find_jump_to_current();
                    }
                    if ui.small_button("\u{25BC} Next").clicked() && !self.find.match_indices.is_empty() {
                        self.find.current_match = (self.find.current_match + 1) % self.find.match_indices.len();
                        self.find_jump_to_current();
                    }
                    if ui.small_button("\u{2715} Close").clicked() {
                        self.find.active = false;
                        self.find.search_text.clear();
                        self.find.regex = None;
                        self.find.match_indices.clear();
                        self.find.current_match = 0;
                    }
                });
            });
        }

        // Status bar
        let mut reconnect_action = false;
        let mut disconnect_action = false;
        let mut connect_action = false;
        let mut stop_tc_action = false;
        let active_tc_id = self
            .active_test_case
            .and_then(|i| self.test_cases.get(i))
            .map(|tc| tc.id.clone());
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.is_connected {
                    ui.colored_label(egui::Color32::GREEN, "\u{25CF} Connected");
                } else if self.is_loading {
                    ui.colored_label(egui::Color32::YELLOW, "\u{25CF} Loading");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "\u{25CF} Idle");
                }

                if let Some(id) = &active_tc_id {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 70, 70),
                        format!("\u{25CF} REC — {}", id),
                    );
                    if ui.small_button("Stop").clicked() {
                        stop_tc_action = true;
                    }
                }

                // Quick action buttons for SSH mode
                if self.last_ssh_config.is_some() {
                    ui.separator();
                    if self.is_connected {
                        if ui.small_button("Disconnect").clicked() {
                            disconnect_action = true;
                        }
                    } else if !self.is_loading {
                        if ui.small_button("Reconnect").clicked() {
                            reconnect_action = true;
                        }
                        if ui.small_button("Connect...").clicked() {
                            connect_action = true;
                        }
                    }
                }

                ui.separator();
                ui.label(&self.status_message);
                ui.separator();
                ui.label(format!(
                    "Showing {} / {} entries",
                    self.filtered_indices.len(),
                    self.log_store.entries.len()
                ));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("PSS: {} MB", self.memory_kb / 1024))
                            .color(egui::Color32::from_rgb(130, 130, 130)),
                    );
                });
            });
        });

        // Handle status bar button actions (outside borrow of ui)
        if disconnect_action {
            self.disconnect();
        }
        if reconnect_action {
            if let Some(config) = self.last_ssh_config.clone() {
                self.start_ssh(config);
            }
        }
        if connect_action {
            self.connection_dialog.open = true;
        }
        if stop_tc_action {
            self.stop_test_case();
        }

        // Handle "Show in Context" request (before rendering panels so data is ready this frame)
        if self.log_viewer.show_in_context_requested {
            self.log_viewer.show_in_context_requested = false;
            if let Some(entry_idx) = self.log_viewer.selected_entry {
                self.saved_filter_bar = Some(self.filter_bar.clone());
                self.filter_bar = FilterBar::default();
                self.filter = FilterCriteria::default();
                self.apply_filter();
                // Find the row index of entry_idx in the now-unfiltered list
                if let Some(row) = self.filtered_indices.iter().position(|&i| i == entry_idx) {
                    self.log_viewer.scroll_to_row = Some(row);
                }
            }
        }

        // "Filters temporarily cleared" banner
        if self.saved_filter_bar.is_some() {
            egui::TopBottomPanel::top("context_banner").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 60),
                        "Filters temporarily cleared — showing all logs",
                    );
                    if ui.button("Restore Filters").clicked() {
                        if let Some(saved) = self.saved_filter_bar.take() {
                            self.filter_bar = saved;
                            self.filter_bar.apply_to_filter(&mut self.filter);
                            self.apply_filter();
                        }
                    }
                });
            });
        }

        // Bookmark / timeline window
        if self.show_bookmarks {
            let mut open = self.show_bookmarks;
            let bookmark_count = self.bookmarks.len();
            let title = format!("Bookmarks ({}) \u{2014} Timeline", bookmark_count);
            let mut remove_idx: Option<usize> = None;
            let mut navigate_idx: Option<usize> = None;

            egui::Window::new(title)
                .open(&mut open)
                .resizable(true)
                .default_size([760.0, 420.0])
                .show(ctx, |ui| {
                    if self.bookmarks.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label("No bookmarks yet.\nRight-click a log entry or press B to bookmark.");
                        });
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button("Clear All").clicked() {
                                self.bookmarks.clear();
                            }
                            ui.label(egui::RichText::new(format!("{} bookmarks — click a row to navigate", bookmark_count))
                                .color(egui::Color32::from_rgb(160, 160, 160)));
                        });
                        ui.separator();

                        // Header
                        ui.horizontal(|ui| {
                            ui.add_sized([55.0, 16.0], egui::Label::new(egui::RichText::new("Line#").strong().monospace()));
                            ui.add_sized([160.0, 16.0], egui::Label::new(egui::RichText::new("Timestamp").strong().monospace()));
                            ui.add_sized([55.0, 16.0], egui::Label::new(egui::RichText::new("Pri").strong().monospace()));
                            ui.add_sized([140.0, 16.0], egui::Label::new(egui::RichText::new("Service").strong().monospace()));
                            ui.label(egui::RichText::new("Message").strong().monospace());
                        });
                        ui.separator();

                        let mut sorted: Vec<usize> = self.bookmarks.iter().copied().collect();
                        sorted.sort();

                        egui::ScrollArea::both().show(ui, |ui| {
                            for &entry_idx in &sorted {
                                if let Some(entry) = self.log_store.entries.get(entry_idx) {
                                    let row = ui.horizontal(|ui| {
                                        ui.add_sized([55.0, 18.0], egui::Label::new(
                                            egui::RichText::new(format!("\u{2605}{}", entry.line_num))
                                                .monospace()
                                                .color(egui::Color32::from_rgb(255, 200, 50)),
                                        ));
                                        ui.add_sized([160.0, 18.0], egui::Label::new(
                                            egui::RichText::new(&entry.timestamp)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(180, 180, 180)),
                                        ));
                                        ui.add_sized([55.0, 18.0], egui::Label::new(
                                            egui::RichText::new(priority_label(entry.priority))
                                                .monospace()
                                                .color(priority_color(entry.priority)),
                                        ));
                                        ui.add_sized([140.0, 18.0], egui::Label::new(
                                            egui::RichText::new(&entry.service)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(130, 200, 255)),
                                        ));
                                        let msg_resp = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&entry.message)
                                                    .monospace()
                                                    .color(egui::Color32::from_rgb(220, 220, 220)),
                                            )
                                            .sense(egui::Sense::click()),
                                        );
                                        if msg_resp.clicked() {
                                            navigate_idx = Some(entry_idx);
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button("\u{2715}").clicked() {
                                                remove_idx = Some(entry_idx);
                                            }
                                        });
                                    });
                                    let row_rect = row.response.rect;
                                    let row_resp = ui.interact(row_rect, ui.id().with(("bm_row", entry_idx)), egui::Sense::click());
                                    if row_resp.clicked() {
                                        navigate_idx = Some(entry_idx);
                                    }
                                    if row_resp.hovered() {
                                        ui.painter().rect_filled(row_rect, 0.0, egui::Color32::from_rgba_premultiplied(50, 50, 65, 100));
                                    }
                                    ui.separator();
                                }
                            }
                        });
                    }
                });

            self.show_bookmarks = open;

            if let Some(idx) = remove_idx {
                self.bookmarks.remove(&idx);
            }
            if let Some(entry_idx) = navigate_idx {
                self.log_viewer.selected_entry = Some(entry_idx);
                if let Some(row) = self.filtered_indices.iter().position(|&i| i == entry_idx) {
                    self.log_viewer.scroll_to_row = Some(row);
                } else {
                    self.status_message = "Bookmarked entry is hidden by the active filter".to_string();
                }
            }
        }

        // Test Cases window
        if self.show_test_cases {
            let mut open = self.show_test_cases;
            let total_entries = self.log_store.entries.len();
            let mut export_idx: Option<usize> = None;
            let mut remove_idx: Option<usize> = None;
            let mut view_range: Option<(usize, usize)> = None;
            let mut export_all = false;
            let mut stop_from_window = false;

            egui::Window::new(format!("Test Cases ({})", self.test_cases.len()))
                .open(&mut open)
                .resizable(true)
                .default_size([720.0, 400.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!self.test_cases.is_empty(), egui::Button::new("Export All"))
                            .on_hover_text("Write each test case to its own file (plus a session log when more than one exists)")
                            .clicked()
                        {
                            export_all = true;
                        }
                        ui.label(egui::RichText::new(
                            "Each test case exports to its own file; the full session log is saved alongside when several ran.",
                        ).small().color(egui::Color32::from_rgb(160, 160, 160)));
                    });
                    ui.separator();

                    if self.test_cases.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label("No test cases yet.\nUse Test Case ▸ Start Test Case… to begin recording.");
                        });
                        return;
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, tc) in self.test_cases.iter().enumerate() {
                            let range = tc.range(total_entries);
                            let count = range.len();
                            let recording = tc.is_recording();

                            ui.horizontal(|ui| {
                                ui.strong(&tc.id);
                                if !tc.name.is_empty() {
                                    ui.label(egui::RichText::new(&tc.name).color(egui::Color32::from_rgb(130, 200, 255)));
                                }
                                if recording {
                                    ui.colored_label(egui::Color32::from_rgb(230, 70, 70), "\u{25CF} recording");
                                }
                            });
                            if !tc.description.is_empty() {
                                ui.label(egui::RichText::new(&tc.description)
                                    .small()
                                    .color(egui::Color32::from_rgb(180, 180, 180)));
                            }
                            ui.horizontal(|ui| {
                                let range_text = if let (Some(first), Some(last)) = (
                                    self.log_store.entries.get(range.start),
                                    range.end.checked_sub(1).and_then(|e| self.log_store.entries.get(e)),
                                ) {
                                    format!("Line# {}–{} · {} entries", first.line_num, last.line_num, count)
                                } else {
                                    format!("{} entries", count)
                                };
                                ui.label(egui::RichText::new(range_text).monospace()
                                    .color(egui::Color32::from_rgb(150, 150, 150)));

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("Remove").clicked() {
                                        remove_idx = Some(i);
                                    }
                                    if recording {
                                        if ui.small_button("Stop").clicked() {
                                            stop_from_window = true;
                                        }
                                    } else {
                                        if ui.add_enabled(count > 0, egui::Button::new("Export").small()).clicked() {
                                            export_idx = Some(i);
                                        }
                                        if let (Some(first), Some(last)) = (
                                            self.log_store.entries.get(range.start),
                                            range.end.checked_sub(1).and_then(|e| self.log_store.entries.get(e)),
                                        ) {
                                            let (fl, ll) = (first.line_num, last.line_num);
                                            if ui.add_enabled(count > 0, egui::Button::new("View").small())
                                                .on_hover_text("Filter the table to this test case's Line# range")
                                                .clicked()
                                            {
                                                view_range = Some((fl, ll));
                                            }
                                        }
                                    }
                                });
                            });
                            ui.separator();
                        }
                    });
                });

            self.show_test_cases = open;

            if stop_from_window {
                self.stop_test_case();
            }
            if let Some(i) = remove_idx {
                if self.active_test_case == Some(i) {
                    self.active_test_case = None;
                } else if let Some(active) = self.active_test_case {
                    if active > i {
                        self.active_test_case = Some(active - 1);
                    }
                }
                self.test_cases.remove(i);
            }
            if let Some(i) = export_idx {
                if let Some(tc) = self.test_cases.get(i).cloned() {
                    match self.export_test_case(&tc) {
                        Ok(path) => self.status_message = format!("Exported test case '{}' to {}", tc.id, path),
                        Err(e) => self.status_message = format!("Export error: {}", e),
                    }
                }
            }
            if export_all {
                self.export_all_test_cases();
            }
            if let Some((from, to)) = view_range {
                self.filter_bar.set_line_range(from, to, &mut self.filter);
                self.apply_filter();
            }
        }

        // Central log viewer
        let find_pattern = self.find.regex.as_ref();
        let current_find_row = if self.find.active && !self.find.match_indices.is_empty() {
            self.find.match_indices.get(self.find.current_match).copied()
        } else {
            None
        };
        let show_context_button = self.saved_filter_bar.is_none() && self.filter_bar.is_active();
        let in_context_mode = self.saved_filter_bar.is_some();
        egui::CentralPanel::default().show(ctx, |ui| {
            self.log_viewer.show(ui, &self.log_store, &self.filtered_indices, &self.filter, find_pattern, current_find_row, show_context_button, in_context_mode, &self.bookmarks);
        });
    }
}
