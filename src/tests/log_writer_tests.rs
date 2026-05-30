    use super::*;
    use crate::analyzer::state::LogEntry;
    use crate::ui::save_settings::{SaveFormat, SaveSettings};

    fn make_entry(ts: &str, svc: &str, pri: u8, msg: &str) -> LogEntry {
        LogEntry {
            line_num: 1,
            timestamp: ts.to_string(),
            priority: pri,
            service: svc.to_string(),
            message: msg.to_string(),
        }
    }

    fn unique_name(label: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        format!("jlog_test_{}_{}_{}", label, std::process::id(), ns)
    }

    fn tmp_settings(label: &str, fmt: SaveFormat) -> SaveSettings {
        SaveSettings {
            destination: "/tmp".to_string(),
            filename_template: unique_name(label),
            format: fmt,
            auto_save: false,
            save_filtered_only: false,
            always_show_bookmarks: false,
        }
    }

    #[test]
    fn save_plaintext_roundtrip() {
        let e1 = make_entry("2026-01-01 10:00:00", "sshd", 6, "Connected");
        let e2 = make_entry("2026-01-01 10:00:01", "kernel", 3, "Error!");
        let settings = tmp_settings("plain", SaveFormat::PlainText);
        let path = save_logs(&[&e1, &e2], &settings, "testhost").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(content.contains("sshd[6]: Connected"));
        assert!(content.contains("kernel[3]: Error!"));
        assert!(content.contains("2026-01-01 10:00:00"));
    }

    #[test]
    fn save_json_roundtrip() {
        let e = make_entry("2026-01-01 10:00:00.123456", "nginx", 4, "warn msg");
        let settings = tmp_settings("json", SaveFormat::Json);
        let path = save_logs(&[&e], &settings, "host").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();

        let parsed: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed["service"], "nginx");
        assert_eq!(parsed["priority"], 4);
        assert_eq!(parsed["message"], "warn msg");
        assert_eq!(parsed["timestamp"], "2026-01-01 10:00:00.123456");
    }

    #[test]
    fn save_empty_entries_creates_file() {
        let settings = tmp_settings("empty", SaveFormat::PlainText);
        let path = save_logs(&[], &settings, "host").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(content.is_empty());
    }

    #[test]
    fn save_creates_destination_dir() {
        let tmp = std::env::temp_dir().join(format!("jlog_test_dir_{}", std::process::id()));
        let settings = SaveSettings {
            destination: tmp.to_string_lossy().to_string(),
            filename_template: "testfile".to_string(),
            format: SaveFormat::PlainText,
            auto_save: false,
            save_filtered_only: false,
            always_show_bookmarks: false,
        };
        let e = make_entry("2026-01-01 00:00:00", "sshd", 6, "hi");
        let path = save_logs(&[&e], &settings, "host").unwrap();
        assert!(std::path::Path::new(&path).exists());
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&tmp).ok();
    }
