    use super::*;

    fn entry(timestamp: Option<&str>, priority: Option<&str>, syslog_id: Option<&str>, unit: Option<&str>, message: Option<&str>) -> JournalEntry {
        JournalEntry {
            realtime_timestamp: timestamp.map(|s| s.to_string()),
            priority: priority.map(|s| s.to_string()),
            syslog_identifier: syslog_id.map(|s| s.to_string()),
            systemd_unit: unit.map(|s| s.to_string()),
            message: message.map(|s| s.to_string()),
        }
    }

    // --- priority_num ---

    #[test]
    fn priority_num_valid() {
        assert_eq!(entry(None, Some("3"), None, None, None).priority_num(), 3);
        assert_eq!(entry(None, Some("0"), None, None, None).priority_num(), 0);
        assert_eq!(entry(None, Some("7"), None, None, None).priority_num(), 7);
    }

    #[test]
    fn priority_num_missing_defaults_to_6() {
        assert_eq!(entry(None, None, None, None, None).priority_num(), 6);
    }

    #[test]
    fn priority_num_invalid_string_defaults_to_6() {
        assert_eq!(entry(None, Some("bad"), None, None, None).priority_num(), 6);
    }

    // --- service ---

    #[test]
    fn service_prefers_syslog_identifier() {
        let e = entry(None, None, Some("sshd"), Some("ssh.service"), None);
        assert_eq!(e.service(), "sshd");
    }

    #[test]
    fn service_falls_back_to_systemd_unit() {
        let e = entry(None, None, None, Some("systemd-journald.service"), None);
        assert_eq!(e.service(), "systemd-journald.service");
    }

    #[test]
    fn service_defaults_to_unknown() {
        let e = entry(None, None, None, None, None);
        assert_eq!(e.service(), "unknown");
    }

    // --- msg ---

    #[test]
    fn msg_returns_message() {
        let e = entry(None, None, None, None, Some("hello world"));
        assert_eq!(e.msg(), "hello world");
    }

    #[test]
    fn msg_empty_when_missing() {
        let e = entry(None, None, None, None, None);
        assert_eq!(e.msg(), "");
    }

    // --- timestamp_micros ---

    #[test]
    fn timestamp_micros_parses_valid() {
        let e = entry(Some("1700000000000000"), None, None, None, None);
        assert_eq!(e.timestamp_micros(), Some(1_700_000_000_000_000i64));
    }

    #[test]
    fn timestamp_micros_none_when_missing() {
        let e = entry(None, None, None, None, None);
        assert!(e.timestamp_micros().is_none());
    }

    #[test]
    fn timestamp_micros_none_when_non_numeric() {
        let e = entry(Some("not-a-number"), None, None, None, None);
        assert!(e.timestamp_micros().is_none());
    }

    // --- infer_priority ---

    #[test]
    fn infer_priority_critical_keywords() {
        assert_eq!(infer_priority("kernel panic occurred"), 2);
        assert_eq!(infer_priority("Fatal error"), 2);
        assert_eq!(infer_priority("CRITICAL failure"), 2);
    }

    #[test]
    fn infer_priority_error_keywords() {
        assert_eq!(infer_priority("connection failed"), 3);
        assert_eq!(infer_priority("Error opening file"), 3);
        assert_eq!(infer_priority("operation failure"), 3);
        assert_eq!(infer_priority("cannot connect"), 3);
        assert_eq!(infer_priority("unable to bind"), 3);
        assert_eq!(infer_priority("segfault at 0x0"), 3);
        assert_eq!(infer_priority("NullPointerException"), 3);
    }

    #[test]
    fn infer_priority_warning_keywords() {
        assert_eq!(infer_priority("this is a warning"), 4);
        assert_eq!(infer_priority("warn: low disk"), 4);
        assert_eq!(infer_priority("connection timeout"), 4);
        assert_eq!(infer_priority("request timed out"), 4);
        assert_eq!(infer_priority("retrying connection"), 4);
        assert_eq!(infer_priority("deprecated API used"), 4);
        assert_eq!(infer_priority("access denied"), 4);
        assert_eq!(infer_priority("connection refused"), 4);
    }

    #[test]
    fn infer_priority_notice_keywords() {
        assert_eq!(infer_priority("service started"), 5);
        assert_eq!(infer_priority("server stopped"), 5);
        assert_eq!(infer_priority("client connected"), 5);
        assert_eq!(infer_priority("peer disconnected"), 5);
        assert_eq!(infer_priority("config loaded"), 5);
        assert_eq!(infer_priority("task finished"), 5);
    }

    #[test]
    fn infer_priority_default_info() {
        assert_eq!(infer_priority("hello world"), 6);
        assert_eq!(infer_priority(""), 6);
        assert_eq!(infer_priority("normal log line"), 6);
    }

    // --- from_syslog_line ---

    #[test]
    fn from_syslog_line_valid() {
        let line = "May 29 10:30:45 myhostname sshd[1234]: Accepted publickey";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.service(), "sshd");
        assert_eq!(e.msg(), "Accepted publickey");
        assert!(e.timestamp_micros().is_some());
    }

    #[test]
    fn from_syslog_line_without_pid() {
        let line = "Jan  1 00:00:01 host kernel: some kernel message";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.service(), "kernel");
        assert_eq!(e.msg(), "some kernel message");
    }

    #[test]
    fn from_syslog_line_garbage_returns_none() {
        assert!(JournalEntry::from_syslog_line("").is_none());
        assert!(JournalEntry::from_syslog_line("not a log line").is_none());
        assert!(JournalEntry::from_syslog_line("{\"json\": true}").is_none());
    }

    #[test]
    fn from_syslog_line_infers_priority_from_message() {
        let line = "May 29 10:30:45 host sshd[1]: connection failed";
        let e = JournalEntry::from_syslog_line(line).unwrap();
        assert_eq!(e.priority_num(), 3); // error keyword
    }

    // --- JSON deserialization ---

    #[test]
    fn deserialize_full_json_entry() {
        let json = r#"{
            "__REALTIME_TIMESTAMP": "1700000000000000",
            "PRIORITY": "4",
            "SYSLOG_IDENTIFIER": "myapp",
            "_SYSTEMD_UNIT": "myapp.service",
            "MESSAGE": "hello"
        }"#;
        let e: JournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.priority_num(), 4);
        assert_eq!(e.service(), "myapp");
        assert_eq!(e.msg(), "hello");
        assert_eq!(e.timestamp_micros(), Some(1_700_000_000_000_000i64));
    }

    #[test]
    fn deserialize_minimal_json_entry() {
        let json = r#"{"MESSAGE": "bare minimum"}"#;
        let e: JournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.msg(), "bare minimum");
        assert_eq!(e.priority_num(), 6);
        assert_eq!(e.service(), "unknown");
        assert!(e.timestamp_micros().is_none());
    }
