use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;

use crate::analyzer::{LogStore, FilterCriteria};
use crate::background::{BackgroundMessage, BackgroundCommand};
use crate::ui::connection_dialog::ConnectionDialog;
use crate::ui::filter_bar::FilterBar;
use crate::ui::log_viewer::LogViewer;
use crate::ui::open_file_dialog::OpenFileDialog;
use crate::ui::save_settings::{SaveSettings, SaveSettingsDialog};
use crate::workers::{file_reader, log_writer, ssh_reader};

pub struct JlogApp {
    log_store: LogStore,
    filter: FilterCriteria,
    filtered_indices: Vec<usize>,

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

    save_settings: SaveSettings,
    save_settings_dialog: SaveSettingsDialog,
    current_host: String,

    /// File to load on first frame (from CLI argument)
    pending_file: Option<String>,
}

impl JlogApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Check for CLI argument: jlog <file>
        let pending_file = std::env::args().nth(1).filter(|p| {
            let path = std::path::Path::new(p);
            path.exists() && path.is_file()
        });

        Self {
            log_store: LogStore::new(),
            filter: FilterCriteria::default(),
            filtered_indices: Vec::new(),

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

            save_settings: SaveSettings::default(),
            save_settings_dialog: SaveSettingsDialog::default(),
            current_host: "local".to_string(),

            pending_file,
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
        self.bg_receiver = None;
        self.bg_cmd_sender = None;
        self.is_loading = false;
        self.is_connected = false;
        self.total_lines = 0;
        self.filter_bar = FilterBar::default();
        self.filter = FilterCriteria::default();
    }

    fn process_messages(&mut self) {
        let receiver = match &self.bg_receiver {
            Some(rx) => rx.clone(),
            None => return,
        };

        let mut new_entries = false;
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
            self.apply_filter();
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

    fn apply_filter(&mut self) {
        self.filtered_indices.clear();
        for (i, entry) in self.log_store.entries.iter().enumerate() {
            if self.filter.matches(entry) {
                self.filtered_indices.push(i);
            }
        }
    }
}

impl eframe::App for JlogApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // Use exact egui panel background to prevent artifacts on resize/maximize
        let c = visuals.panel_fill;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Always repaint — prevents stale frame artifacts on resize/maximize (WSL2/X11)
        ctx.request_repaint();

        // Load file from CLI argument on first frame
        if let Some(path) = self.pending_file.take() {
            self.load_file(path);
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
            self.save_settings = new_settings;
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open File...").clicked() {
                        ui.close_menu();
                        self.open_file_dialog.open = true;
                    }
                    if ui.button("Connect SSH...").clicked() {
                        ui.close_menu();
                        self.connection_dialog.open = true;
                    }
                    ui.separator();
                    if ui.button("Save Logs Now").clicked() {
                        ui.close_menu();
                        self.save_now();
                    }
                    if ui.button("Save Settings...").clicked() {
                        ui.close_menu();
                        self.save_settings_dialog.load_from(&self.save_settings);
                        self.save_settings_dialog.open = true;
                    }
                    ui.separator();
                    if self.is_connected {
                        if ui.button("Disconnect").clicked() {
                            ui.close_menu();
                            self.disconnect();
                        }
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.log_viewer.auto_scroll, "Auto-scroll");
                });
            });
        });

        // Filter bar panel
        egui::TopBottomPanel::top("filter_bar").show(ctx, |ui| {
            let services = self.log_store.service_names();
            if self.filter_bar.show(ui, &services, &mut self.filter) {
                self.apply_filter();
            }
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.is_connected {
                    ui.colored_label(egui::Color32::GREEN, "\u{25CF} Connected");
                } else if self.is_loading {
                    ui.colored_label(egui::Color32::YELLOW, "\u{25CF} Loading");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "\u{25CF} Idle");
                }

                ui.separator();
                ui.label(&self.status_message);
                ui.separator();
                ui.label(format!(
                    "Showing {} / {} entries",
                    self.filtered_indices.len(),
                    self.log_store.entries.len()
                ));
            });
        });

        // Central log viewer
        egui::CentralPanel::default().show(ctx, |ui| {
            self.log_viewer.show(ui, &self.log_store, &self.filtered_indices, &self.filter);
        });
    }
}
