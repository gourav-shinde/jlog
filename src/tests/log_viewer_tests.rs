    use super::*;
    use crate::analyzer::{LogStore, FilterCriteria};

    fn make_entry(line: usize, svc: &str, pri: u8, msg: &str) -> LogEntry {
        LogEntry {
            line_num: line,
            timestamp: "2026-01-01 00:00:00".to_string(),
            priority: pri,
            service: svc.to_string(),
            message: msg.to_string(),
        }
    }

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    // --- priority_label ---

    #[test]
    fn priority_label_all() {
        assert_eq!(priority_label(0), "EMERG");
        assert_eq!(priority_label(1), "ALERT");
        assert_eq!(priority_label(2), "CRIT");
        assert_eq!(priority_label(3), "ERR");
        assert_eq!(priority_label(4), "WARN");
        assert_eq!(priority_label(5), "NOTICE");
        assert_eq!(priority_label(6), "INFO");
        assert_eq!(priority_label(7), "DEBUG");
        assert_eq!(priority_label(255), "???");
    }

    // --- priority_color ---

    #[test]
    fn priority_color_by_level() {
        let colors: Vec<_> = (0u8..=8).map(priority_color).collect();
        assert_eq!(colors[0], colors[1]);
        assert_eq!(colors[1], colors[2]);
        assert_ne!(colors[3], colors[4]);
    }

    // --- try_pretty_json ---

    #[test]
    fn try_pretty_json_valid_object() {
        let result = try_pretty_json(r#"{"key":"val"}"#);
        assert!(result.is_some());
        assert!(result.unwrap().contains('\n'));
    }

    #[test]
    fn try_pretty_json_valid_array() {
        assert!(try_pretty_json("[1,2,3]").is_some());
    }

    #[test]
    fn try_pretty_json_plain_text_returns_none() {
        assert!(try_pretty_json("just a plain message").is_none());
        assert!(try_pretty_json("").is_none());
    }

    #[test]
    fn try_pretty_json_invalid_json_returns_none() {
        assert!(try_pretty_json("{bad json}").is_none());
    }

    // --- format_entry_for_copy ---

    #[test]
    fn format_entry_for_copy_includes_all_fields() {
        let e = make_entry(42, "sshd", 6, "Connected");
        let s = format_entry_for_copy(&e);
        assert!(s.contains("2026-01-01 00:00:00"));
        assert!(s.contains("sshd"));
        assert!(s.contains("INFO"));
        assert!(s.contains("Connected"));
    }

    // --- notify_new_entries ---

    #[test]
    fn notify_new_entries_no_change_when_auto_scroll_on() {
        let mut v = LogViewer::default();
        v.auto_scroll = true;
        v.notify_new_entries(10);
        assert_eq!(v.new_entry_count, 0);
    }

    #[test]
    fn notify_new_entries_accumulates_when_not_at_bottom() {
        let mut v = LogViewer::default();
        v.auto_scroll = false;
        v.is_at_bottom = false;
        v.notify_new_entries(5);
        v.notify_new_entries(3);
        assert_eq!(v.new_entry_count, 8);
    }

    #[test]
    fn notify_new_entries_no_change_when_at_bottom() {
        let mut v = LogViewer::default();
        v.auto_scroll = false;
        v.is_at_bottom = true;
        v.notify_new_entries(10);
        assert_eq!(v.new_entry_count, 0);
    }

    // --- headless egui show() ---

    #[test]
    fn show_empty_store_renders_without_panic() {
        let store = LogStore::new();
        let filtered: Vec<usize> = vec![];
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_entries_renders_without_panic() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "Connected"));
        store.entries.push(make_entry(2, "kernel", 3, "Error occurred"));
        store.entries.push(make_entry(3, "nginx", 4, "Warning: slow response"));
        let filtered = vec![0, 1, 2];
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_selected_entry_renders_detail_panel() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "Connected"));
        let filtered = vec![0];
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        viewer.selected_entry = Some(0);
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_json_message_renders_pretty() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "app", 6, r#"{"key":"value","num":42}"#));
        let filtered = vec![0];
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        viewer.selected_entry = Some(0);
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_filter_pattern_renders_highlights() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "auth error in connection"));
        let filtered = vec![0];
        let mut filter = FilterCriteria::default();
        filter.set_pattern("error");
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_bookmark_renders_star_icon() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "bookmarked line"));
        let filtered = vec![0];
        let filter = FilterCriteria::default();
        let mut bookmarks = std::collections::HashSet::new();
        bookmarks.insert(0usize);
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_context_mode() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "log entry"));
        let filtered = vec![0];
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        viewer.selected_entry = Some(0);
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, true, true, &bookmarks);
            });
        });
    }

    #[test]
    fn show_all_priority_levels() {
        let mut store = LogStore::new();
        for pri in 0u8..=7 {
            store.entries.push(make_entry(pri as usize + 1, "svc", pri, "msg"));
        }
        let filtered: Vec<usize> = (0..8).collect();
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_scroll_to_row_set() {
        let mut store = LogStore::new();
        for i in 0..10 {
            store.entries.push(make_entry(i + 1, "svc", 6, "msg"));
        }
        let filtered: Vec<usize> = (0..10).collect();
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        viewer.scroll_to_row = Some(5);
        viewer.auto_scroll = false;
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
        assert!(viewer.scroll_to_row.is_none());
    }

    #[test]
    fn show_with_new_entry_count_shows_indicator() {
        let mut store = LogStore::new();
        for i in 0..5 {
            store.entries.push(make_entry(i + 1, "svc", 6, "msg"));
        }
        let filtered: Vec<usize> = (0..5).collect();
        let filter = FilterCriteria::default();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        viewer.auto_scroll = false;
        viewer.is_at_bottom = false;
        viewer.new_entry_count = 7;
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, None, None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_find_pattern_highlights() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "auth error in connection"));
        let filtered = vec![0];
        let filter = FilterCriteria::default();
        let find_re = regex::Regex::new("error").unwrap();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, Some(&find_re), None, false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_find_pattern_and_current_row() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "connection established"));
        store.entries.push(make_entry(2, "kernel", 3, "error in module"));
        let filtered = vec![0, 1];
        let filter = FilterCriteria::default();
        let find_re = regex::Regex::new("error").unwrap();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, Some(&find_re), Some(1), false, false, &bookmarks);
            });
        });
    }

    #[test]
    fn show_with_combined_filter_and_find() {
        let mut store = LogStore::new();
        store.entries.push(make_entry(1, "sshd", 6, "auth error warning"));
        let filtered = vec![0];
        let mut filter = FilterCriteria::default();
        filter.set_pattern("auth");
        let find_re = regex::Regex::new("error").unwrap();
        let bookmarks = std::collections::HashSet::new();
        let mut viewer = LogViewer::default();
        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                viewer.show(ui, &store, &filtered, &filter, Some(&find_re), None, false, false, &bookmarks);
            });
        });
    }
