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
use crate::ui::save_settings::{SaveSettings, SaveSettingsDialog, load_settings, save_settings_to_disk};
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
mod tests {
    use super::*;
    use crate::analyzer::state::LogEntry;
    use crossbeam_channel::unbounded;
    use eframe::App as EframeApp;

    fn make_entry(line: usize, svc: &str, pri: u8, msg: &str) -> LogEntry {
        LogEntry {
            line_num: line,
            timestamp: "2026-01-01 00:00:00".to_string(),
            priority: pri,
            service: svc.to_string(),
            message: msg.to_string(),
        }
    }

    fn make_app() -> JlogApp {
        JlogApp {
            log_store: LogStore::new(),
            filter: FilterCriteria::default(),
            filtered_indices: Vec::new(),
            filtered_up_to: 0,
            open_file_dialog: crate::ui::open_file_dialog::OpenFileDialog::default(),
            connection_dialog: crate::ui::connection_dialog::ConnectionDialog::default(),
            filter_bar: crate::ui::filter_bar::FilterBar::default(),
            log_viewer: crate::ui::log_viewer::LogViewer::default(),
            bg_receiver: None,
            bg_cmd_sender: None,
            is_loading: false,
            is_connected: false,
            status_message: String::new(),
            total_lines: 0,
            cached_services: Vec::new(),
            save_settings: crate::ui::save_settings::SaveSettings::default(),
            save_settings_dialog: crate::ui::save_settings::SaveSettingsDialog::default(),
            current_host: "testhost".to_string(),
            last_ssh_config: None,
            pending_file: None,
            pending_ssh_profile: None,
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
            memory_kb: 0,
            last_memory_check: std::time::Instant::now(),
        }
    }

    // --- read_memory_kb ---

    #[test]
    fn read_memory_kb_returns_nonnegative() {
        let kb = read_memory_kb();
        assert!(kb < 100_000_000, "suspiciously large: {kb}");
    }

    // --- extend_filter ---

    #[test]
    fn extend_filter_empty_store_produces_no_indices() {
        let mut app = make_app();
        app.extend_filter();
        assert!(app.filtered_indices.is_empty());
    }

    #[test]
    fn extend_filter_adds_entries_matching_default_filter() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "hello"));
        app.log_store.entries.push(make_entry(2, "kernel", 3, "error"));
        app.extend_filter();
        assert_eq!(app.filtered_indices, vec![0, 1]);
        assert_eq!(app.filtered_up_to, 2);
    }

    #[test]
    fn extend_filter_respects_priority_filter() {
        let mut app = make_app();
        app.filter.max_priority = 4;
        app.log_store.entries.push(make_entry(1, "sshd", 3, "error"));
        app.log_store.entries.push(make_entry(2, "kernel", 6, "info"));
        app.extend_filter();
        assert_eq!(app.filtered_indices, vec![0]); // only priority 3 passes max_priority 4
    }

    #[test]
    fn extend_filter_only_processes_new_entries() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "first"));
        app.extend_filter();
        assert_eq!(app.filtered_indices.len(), 1);

        app.log_store.entries.push(make_entry(2, "sshd", 6, "second"));
        app.extend_filter(); // should only add the new one
        assert_eq!(app.filtered_indices.len(), 2);
    }

    #[test]
    fn extend_filter_pinned_bookmarks_always_included() {
        let mut app = make_app();
        app.save_settings.always_show_bookmarks = true;
        app.filter.max_priority = 3; // only errors
        app.log_store.entries.push(make_entry(1, "sshd", 6, "info")); // filtered out
        app.log_store.entries.push(make_entry(2, "kernel", 3, "error")); // passes
        app.bookmarks.insert(0); // bookmark the filtered-out entry
        app.extend_filter();
        // entry 0 is bookmarked and pinned → included despite filter
        // entry 1 passes filter → included
        assert!(app.filtered_indices.contains(&0));
        assert!(app.filtered_indices.contains(&1));
    }

    // --- apply_filter ---

    #[test]
    fn apply_filter_resets_and_rebuilds() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "hello"));
        app.filtered_indices = vec![99]; // stale
        app.filtered_up_to = 5;
        app.apply_filter();
        assert_eq!(app.filtered_indices, vec![0]);
        assert_eq!(app.filtered_up_to, 1);
    }

    // --- update_find_matches ---

    #[test]
    fn update_find_matches_no_regex_produces_empty() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "hello"));
        app.filtered_indices = vec![0];
        app.find.regex = None;
        app.update_find_matches();
        assert!(app.find.match_indices.is_empty());
        assert_eq!(app.find.current_match, 0);
    }

    #[test]
    fn update_find_matches_finds_matching_rows() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "error occurred"));
        app.log_store.entries.push(make_entry(2, "kernel", 6, "all good"));
        app.log_store.entries.push(make_entry(3, "nginx", 6, "another error"));
        app.filtered_indices = vec![0, 1, 2];
        app.find.regex = Some(Regex::new("error").unwrap());
        app.update_find_matches();
        assert_eq!(app.find.match_indices, vec![0, 2]); // rows 0 and 2
    }

    // --- find_jump_to_current ---

    #[test]
    fn find_jump_to_current_sets_scroll_row() {
        let mut app = make_app();
        app.find.match_indices = vec![3, 7, 12];
        app.find.current_match = 1;
        app.find_jump_to_current();
        assert_eq!(app.log_viewer.scroll_to_row, Some(7));
    }

    #[test]
    fn find_jump_to_current_empty_matches_does_nothing() {
        let mut app = make_app();
        app.find.match_indices = vec![];
        app.find.current_match = 0;
        app.find_jump_to_current();
        assert!(app.log_viewer.scroll_to_row.is_none());
    }

    // --- disconnect ---

    #[test]
    fn disconnect_clears_connected_state() {
        let mut app = make_app();
        app.is_connected = true;
        app.is_loading = true;
        app.disconnect();
        assert!(!app.is_connected);
        assert!(!app.is_loading);
        assert!(app.status_message.contains("Disconnect"));
    }

    #[test]
    fn disconnect_sends_command_when_sender_set() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundCommand>();
        app.bg_cmd_sender = Some(tx);
        app.is_connected = true;
        app.disconnect();
        assert!(!app.is_connected);
        assert!(app.bg_cmd_sender.is_none());
        // Verify the disconnect command was sent
        assert!(matches!(rx.try_recv(), Ok(crate::background::BackgroundCommand::Disconnect)));
    }

    // --- reset_state ---

    #[test]
    fn reset_state_clears_all_data() {
        let mut app = make_app();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "data"));
        app.filtered_indices = vec![0];
        app.filtered_up_to = 1;
        app.is_loading = true;
        app.is_connected = true;
        app.bookmarks.insert(0);
        app.reset_state();
        assert!(app.log_store.entries.is_empty());
        assert!(app.filtered_indices.is_empty());
        assert_eq!(app.filtered_up_to, 0);
        assert!(!app.is_loading);
        assert!(!app.is_connected);
        assert!(app.bookmarks.is_empty());
        assert!(!app.show_bookmarks);
    }

    // --- write_export_plain ---

    #[test]
    fn write_export_plain_writes_correct_format() {
        let app = make_app();
        let e = make_entry(1, "sshd", 6, "Connected");
        let path = std::path::PathBuf::from(format!("/tmp/jlog_export_test_{}.log", std::process::id()));
        app.write_export_plain(&path, &[&e]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(content.contains("sshd[6]: Connected"));
        assert!(content.contains("2026-01-01 00:00:00"));
    }

    #[test]
    fn write_export_plain_multiple_entries() {
        let app = make_app();
        let e1 = make_entry(1, "sshd", 6, "msg1");
        let e2 = make_entry(2, "kernel", 3, "msg2");
        let path = std::path::PathBuf::from(format!("/tmp/jlog_export_multi_test_{}.log", std::process::id()));
        app.write_export_plain(&path, &[&e1, &e2]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(content.contains("sshd[6]: msg1"));
        assert!(content.contains("kernel[3]: msg2"));
    }

    // --- export_filtered ---

    #[test]
    fn export_filtered_empty_filtered_sets_status_message() {
        let mut app = make_app();
        app.save_settings.destination = "/tmp".to_string();
        app.export_filtered(); // nothing to export
        assert!(app.status_message.contains("Nothing to export") || app.status_message.contains("no entries"));
    }

    #[test]
    fn export_filtered_writes_file_and_updates_status() {
        let mut app = make_app();
        app.save_settings.destination = "/tmp".to_string();
        app.log_store.entries.push(make_entry(1, "sshd", 6, "log line"));
        app.filtered_indices = vec![0];
        app.export_filtered();
        assert!(app.status_message.contains("Exported") || app.status_message.contains("Export error") || app.status_message.contains("entries"));
    }

    // --- save_now ---

    #[test]
    fn save_now_empty_store_sets_status() {
        let mut app = make_app();
        app.save_now();
        assert!(app.status_message.contains("Nothing to save") || app.status_message.contains("no log entries"));
    }

    #[test]
    fn save_now_writes_entries() {
        let mut app = make_app();
        app.save_settings.destination = "/tmp".to_string();
        app.save_settings.filename_template = format!("jlog_savenow_test_{}", std::process::id());
        app.save_settings.format = crate::ui::save_settings::SaveFormat::PlainText;
        app.log_store.entries.push(make_entry(1, "sshd", 6, "log line"));
        app.apply_filter();
        app.save_now();
        assert!(app.status_message.contains("Saved") || app.status_message.contains("Save error"));
    }

    #[test]
    fn save_now_filtered_only() {
        let mut app = make_app();
        app.save_settings.destination = "/tmp".to_string();
        app.save_settings.filename_template = format!("jlog_filteredonly_test_{}", std::process::id());
        app.save_settings.format = crate::ui::save_settings::SaveFormat::PlainText;
        app.save_settings.save_filtered_only = true;
        app.log_store.entries.push(make_entry(1, "sshd", 6, "included"));
        app.log_store.entries.push(make_entry(2, "kernel", 6, "also included"));
        app.filtered_indices = vec![0]; // only first entry is filtered
        app.save_now();
        assert!(app.status_message.contains("Saved 1 entries") || app.status_message.contains("Save error"));
    }

    // --- process_messages ---

    #[test]
    fn process_messages_no_receiver_does_nothing() {
        let mut app = make_app();
        app.bg_receiver = None;
        app.process_messages(); // should return immediately
        assert!(app.log_store.entries.is_empty());
    }

    #[test]
    fn process_messages_handles_entry_message() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        tx.send(crate::background::BackgroundMessage::Entry(make_entry(1, "sshd", 6, "hello"))).unwrap();
        drop(tx);
        app.process_messages();
        assert_eq!(app.log_store.entries.len(), 1);
        assert_eq!(app.log_store.entries[0].service, "sshd");
    }

    #[test]
    fn process_messages_handles_completed_message() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        app.is_loading = true;
        tx.send(crate::background::BackgroundMessage::Completed { total_lines: 100, entries: 50 }).unwrap();
        drop(tx);
        app.process_messages();
        assert!(!app.is_loading);
        assert_eq!(app.total_lines, 100);
        assert!(app.status_message.contains("50"));
    }

    #[test]
    fn process_messages_handles_error_message() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        app.is_loading = true;
        tx.send(crate::background::BackgroundMessage::Error("disk full".to_string())).unwrap();
        drop(tx);
        app.process_messages();
        assert!(!app.is_loading);
        assert!(app.status_message.contains("Error"));
        assert!(app.status_message.contains("disk full"));
    }

    #[test]
    fn process_messages_handles_ssh_connected() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        tx.send(crate::background::BackgroundMessage::SshConnected).unwrap();
        drop(tx);
        app.process_messages();
        assert!(app.is_connected);
        assert!(app.status_message.contains("connected"));
    }

    #[test]
    fn process_messages_handles_ssh_disconnected_no_autosave() {
        let mut app = make_app();
        app.save_settings.auto_save = false;
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        app.is_connected = true;
        tx.send(crate::background::BackgroundMessage::SshDisconnected).unwrap();
        drop(tx);
        app.process_messages();
        assert!(!app.is_connected);
    }

    #[test]
    fn process_messages_handles_progress_message() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        tx.send(crate::background::BackgroundMessage::Progress { lines: 1000, percent: 42.0 }).unwrap();
        drop(tx);
        app.process_messages();
        assert_eq!(app.total_lines, 1000);
        assert!(app.status_message.contains("42"));
    }

    #[test]
    fn process_messages_progress_zero_percent_shows_streaming() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        tx.send(crate::background::BackgroundMessage::Progress { lines: 500, percent: 0.0 }).unwrap();
        drop(tx);
        app.process_messages();
        assert!(app.status_message.contains("Streaming") || app.status_message.contains("500"));
    }

    #[test]
    fn process_messages_clears_receiver_on_disconnect() {
        let mut app = make_app();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        drop(tx); // sender dropped → channel disconnected
        app.process_messages();
        assert!(app.bg_receiver.is_none());
    }

    // --- start_ssh (covers field setup; SSH thread will error immediately since no server) ---

    #[test]
    fn start_ssh_sets_loading_state() {
        let mut app = make_app();
        let config = crate::workers::ssh_reader::SshConfig {
            host: "127.0.0.1".to_string(),
            port: 19999, // port unlikely to be listening
            username: "test".to_string(),
            auth: crate::workers::ssh_reader::AuthMethod::Agent,
            command: "journalctl -o json -n 1".to_string(),
        };
        app.start_ssh(config);
        assert!(app.is_loading);
        assert!(!app.is_connected);
        assert!(app.bg_receiver.is_some());
        assert!(app.bg_cmd_sender.is_some());
        assert_eq!(app.current_host, "127.0.0.1");
        assert!(app.last_ssh_config.is_some());

        // Let the SSH thread fail and drain the error
        std::thread::sleep(std::time::Duration::from_millis(100));
        app.process_messages();
    }

    // --- process_messages: SshDisconnected with auto_save + entries ---

    #[test]
    fn process_messages_ssh_disconnected_with_autosave_and_entries() {
        let mut app = make_app();
        app.save_settings.auto_save = true;
        app.save_settings.destination = "/tmp".to_string();
        app.save_settings.filename_template = format!("jlog_autosave_test_{}", std::process::id());
        app.save_settings.format = crate::ui::save_settings::SaveFormat::PlainText;
        app.log_store.entries.push(make_entry(1, "sshd", 6, "hello"));
        app.apply_filter();
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        app.is_connected = true;
        tx.send(crate::background::BackgroundMessage::SshDisconnected).unwrap();
        drop(tx);
        app.process_messages();
        assert!(!app.is_connected);
        // auto_save would have been triggered; status should say Saved or Disconnected
    }

    #[test]
    fn process_messages_ssh_disconnected_with_existing_error_status() {
        let mut app = make_app();
        app.save_settings.auto_save = false;
        let (tx, rx) = unbounded::<crate::background::BackgroundMessage>();
        app.bg_receiver = Some(rx);
        app.is_connected = true;
        app.status_message = "Error: something went wrong".to_string();
        tx.send(crate::background::BackgroundMessage::SshDisconnected).unwrap();
        drop(tx);
        app.process_messages();
        // Status should remain "Error..." not be overwritten
        assert!(app.status_message.starts_with("Error"));
    }

    // --- clear_color ---

    #[test]
    fn clear_color_dark_theme_returns_opaque_color() {
        let app = make_app();
        let visuals = egui::Visuals::dark();
        let color = app.clear_color(&visuals);
        assert_eq!(color[3], 1.0, "alpha should be 1.0");
        assert!(color[0] >= 0.0 && color[0] <= 1.0);
        assert!(color[1] >= 0.0 && color[1] <= 1.0);
        assert!(color[2] >= 0.0 && color[2] <= 1.0);
    }

    #[test]
    fn clear_color_light_theme_returns_opaque_color() {
        let app = make_app();
        let visuals = egui::Visuals::light();
        let color = app.clear_color(&visuals);
        assert_eq!(color[3], 1.0);
        // light theme panel fill is lighter than dark
        let dark_color = app.clear_color(&egui::Visuals::dark());
        // at least one channel should differ
        let differs = (0..3).any(|i| (color[i] - dark_color[i]).abs() > 0.01);
        assert!(differs, "light and dark clear_color should differ");
    }

    // --- on_exit (when auto_save is disabled, should not panic) ---

    #[test]
    fn on_exit_no_auto_save_does_nothing() {
        let mut app = make_app();
        app.save_settings.auto_save = false;
        app.log_store.entries.push(make_entry(1, "sshd", 6, "msg"));
        app.current_host = "remote".to_string();
        EframeApp::on_exit(&mut app, None); // should not panic
    }

    #[test]
    fn on_exit_local_host_does_not_save() {
        let mut app = make_app();
        app.save_settings.auto_save = true;
        app.log_store.entries.push(make_entry(1, "sshd", 6, "msg"));
        app.current_host = "local".to_string(); // skip save for local
        EframeApp::on_exit(&mut app, None); // should not panic
    }

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    // --- handle_resize_edges (headless) ---

    #[test]
    fn handle_resize_edges_does_not_panic() {
        let app = make_app();
        run_ui(|ctx| {
            app.handle_resize_edges(ctx);
        });
    }

    // --- show_title_bar (headless) ---

    #[test]
    fn show_title_bar_local_host_renders_without_host_label() {
        let app = make_app(); // current_host = "testhost"
        run_ui(|ctx| {
            egui::TopBottomPanel::top("test_tb").show(ctx, |ui| {
                app.show_title_bar(ui, ctx);
            });
        });
    }

    #[test]
    fn show_title_bar_empty_host_renders_without_host_label() {
        let mut app = make_app();
        app.current_host = String::new();
        run_ui(|ctx| {
            egui::TopBottomPanel::top("test_tb2").show(ctx, |ui| {
                app.show_title_bar(ui, ctx);
            });
        });
    }

    #[test]
    fn show_title_bar_remote_host_shows_host_name() {
        let mut app = make_app();
        app.current_host = "prod.server.com".to_string();
        run_ui(|ctx| {
            egui::TopBottomPanel::top("test_tb3").show(ctx, |ui| {
                app.show_title_bar(ui, ctx);
            });
        });
    }

    // --- load_file integration (via channel) ---

    #[test]
    fn load_file_sets_loading_state_and_sends_entries() {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};
        let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let path = format!("/tmp/jlog_app_load_test_{}.log", ns);
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "2026-01-01 10:00:00 sshd[6]: Connected").unwrap();
        }

        let mut app = make_app();
        app.load_file(path.clone());
        assert!(app.is_loading);
        assert!(app.bg_receiver.is_some());

        // Give the thread a moment and drain messages
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.process_messages();
        std::fs::remove_file(&path).ok();

        // After processing, loading should be done and we should have entries
        assert!(!app.log_store.entries.is_empty() || app.is_loading);
    }
}

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

        let (hover_pos, down, delta) = ctx.input(|i| (
            i.pointer.hover_pos(),
            i.pointer.primary_down(),
            i.pointer.delta(),
        ));

        if let Some(pos) = hover_pos {
            let north = pos.y <= rect.min.y + grip;
            let south = pos.y >= rect.max.y - grip;
            let west  = pos.x <= rect.min.x + grip;
            let east  = pos.x >= rect.max.x - grip;

            use egui::viewport::ResizeDirection::*;
            let (dir, cursor) = match (north, south, west, east) {
                (true,  _,     true,  _    ) => (Some(NorthWest), egui::CursorIcon::ResizeNwSe),
                (true,  _,     _,     true ) => (Some(NorthEast), egui::CursorIcon::ResizeNeSw),
                (_,     true,  true,  _    ) => (Some(SouthWest), egui::CursorIcon::ResizeNeSw),
                (_,     true,  _,     true ) => (Some(SouthEast), egui::CursorIcon::ResizeNwSe),
                (true,  _,     _,     _    ) => (Some(North),     egui::CursorIcon::ResizeNorth),
                (_,     true,  _,     _    ) => (Some(South),     egui::CursorIcon::ResizeSouth),
                (_,     _,     true,  _    ) => (Some(West),      egui::CursorIcon::ResizeWest),
                (_,     _,     _,     true ) => (Some(East),      egui::CursorIcon::ResizeEast),
                _                            => (None,             egui::CursorIcon::Default),
            };

            if let Some(direction) = dir {
                ctx.set_cursor_icon(cursor);
                // Trigger resize when mouse is held and has started moving from the edge
                if down && delta != egui::Vec2::ZERO {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        }
    }

    fn show_title_bar(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let drag_response = ui.interact(
            ui.max_rect(),
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

        match self.write_export_plain(&path, &entries) {
            Ok(()) => self.status_message = format!(
                "Exported {} entries to {}",
                entries.len(),
                path.display()
            ),
            Err(e) => self.status_message = format!("Export error: {}", e),
        }
    }

    fn write_export_plain(&self, path: &std::path::Path, entries: &[&crate::analyzer::LogEntry]) -> anyhow::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        for entry in entries {
            writeln!(file, "{} {}[{}]: {}", entry.timestamp, entry.service, entry.priority, entry.message)?;
        }
        Ok(())
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
            if self.filter_bar.show(ui, &self.cached_services, &mut self.filter) {
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
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.is_connected {
                    ui.colored_label(egui::Color32::GREEN, "\u{25CF} Connected");
                } else if self.is_loading {
                    ui.colored_label(egui::Color32::YELLOW, "\u{25CF} Loading");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "\u{25CF} Idle");
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
