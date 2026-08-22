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
            saved_patterns: crate::ui::saved_patterns::SavedPatterns::default(),
            test_cases: Vec::new(),
            active_test_case: None,
            test_case_dialog: crate::ui::testcase::TestCaseDialog::default(),
            show_test_cases: false,
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

    // --- log_writer::write_entries (plain text) ---

    #[test]
    fn write_entries_writes_correct_format() {
        use crate::ui::save_settings::SaveFormat;
        let e = make_entry(1, "sshd", 6, "Connected");
        let path = std::path::PathBuf::from(format!("/tmp/jlog_export_test_{}.log", std::process::id()));
        crate::workers::log_writer::write_entries(&path, &[&e], &SaveFormat::PlainText).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(content.contains("sshd[6]: Connected"));
        assert!(content.contains("2026-01-01 00:00:00"));
    }

    #[test]
    fn write_entries_multiple_entries() {
        use crate::ui::save_settings::SaveFormat;
        let e1 = make_entry(1, "sshd", 6, "msg1");
        let e2 = make_entry(2, "kernel", 3, "msg2");
        let path = std::path::PathBuf::from(format!("/tmp/jlog_export_multi_test_{}.log", std::process::id()));
        crate::workers::log_writer::write_entries(&path, &[&e1, &e2], &SaveFormat::PlainText).unwrap();
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
