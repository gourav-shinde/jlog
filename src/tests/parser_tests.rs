    use super::*;
    use crate::journalctl::JournalEntry;

    // --- parse_log_line (public entry point, end-to-end per format) ---

    #[test]
    fn parse_log_line_journal_json() {
        let json = r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","SYSLOG_IDENTIFIER":"sshd","MESSAGE":"hello"}"#;
        let entry = parse_log_line(json, 1).unwrap();
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.message, "hello");
    }

    #[test]
    fn parse_log_line_bsd_syslog() {
        let entry = parse_log_line("May 29 10:30:45 host sshd[1234]: Accepted publickey", 1).unwrap();
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.message, "Accepted publickey");
    }

    #[test]
    fn parse_log_line_saved_plaintext() {
        let entry = parse_log_line("2026-02-11 10:30:45 sshd[6]: Connected", 1).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45");
        assert_eq!(entry.service, "sshd");
    }

    #[test]
    fn parse_log_line_iso() {
        let entry = parse_log_line("2024-01-15T10:30:45.123Z application started", 1).unwrap();
        assert_eq!(entry.timestamp, "2024-01-15 10:30:45.123Z");
        assert!(entry.service.is_empty());
        assert_eq!(entry.message, "application started");
    }

    #[test]
    fn parse_log_line_raw_fallback() {
        let entry = parse_log_line("totally unstructured text", 9).unwrap();
        assert_eq!(entry.line_num, 9);
        assert_eq!(entry.message, "totally unstructured text");
        assert!(entry.timestamp.is_empty());
        assert!(entry.service.is_empty());
    }

    #[test]
    fn parse_log_line_blank_is_none() {
        assert!(parse_log_line("", 1).is_none());
        assert!(parse_log_line("   ", 1).is_none());
    }

    #[test]
    fn parse_log_line_arbitrary_json_is_kept_raw() {
        // An app's structured JSON log isn't our journal/saved format; rather
        // than swallow it into an empty entry, it's preserved verbatim.
        let line = r#"{"level":"info","msg":"hi"}"#;
        let entry = parse_log_line(line, 1).unwrap();
        assert_eq!(entry.message, line);
        assert!(entry.timestamp.is_empty());
    }

    // --- parse_journal_line ---

    #[test]
    fn parse_journal_line_valid_json() {
        let json = r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","SYSLOG_IDENTIFIER":"sshd","MESSAGE":"hello"}"#;
        let entry = parse_journal_line(json).unwrap();
        assert_eq!(entry.service(), "sshd");
        assert_eq!(entry.msg(), "hello");
    }

    #[test]
    fn parse_journal_line_syslog_format() {
        let entry = parse_journal_line("May 29 10:30:45 host sshd[1234]: Accepted publickey").unwrap();
        assert_eq!(entry.service(), "sshd");
        assert_eq!(entry.msg(), "Accepted publickey");
    }

    #[test]
    fn parse_journal_line_garbage_returns_none() {
        assert!(parse_journal_line("not a log line").is_none());
    }

    #[test]
    fn parse_journal_line_invalid_json_returns_none() {
        assert!(parse_journal_line("{not valid json}").is_none());
    }

    #[test]
    fn parse_journal_line_rejects_field_less_json() {
        // No MESSAGE / __REALTIME_TIMESTAMP -> not accepted as a journal line.
        assert!(parse_journal_line(r#"{"level":"info"}"#).is_none());
    }

    // --- parse_saved_line ---

    #[test]
    fn saved_plaintext_seconds_precision() {
        let entry = parse_saved_line("2026-02-11 10:30:45 sshd[6]: Connected", 1).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45");
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.priority, 6);
        assert_eq!(entry.message, "Connected");
    }

    #[test]
    fn saved_plaintext_microsecond_precision() {
        let entry = parse_saved_line("2026-02-11 10:30:45.123456 sshd[6]: Connected", 1).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45.123456");
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.message, "Connected");
    }

    #[test]
    fn saved_plaintext_empty_message() {
        let entry = parse_saved_line("2026-02-11 10:30:45.000001 kernel[3]: ", 2).unwrap();
        assert_eq!(entry.service, "kernel");
        assert_eq!(entry.priority, 3);
        assert_eq!(entry.message, "");
    }

    #[test]
    fn saved_json_roundtrip() {
        let line = r#"{"line":1,"timestamp":"2026-02-11 10:30:45.123456","priority":6,"service":"sshd","message":"Connected"}"#;
        let entry = parse_saved_line(line, 5).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45.123456");
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.priority, 6);
        assert_eq!(entry.message, "Connected");
    }

    #[test]
    fn saved_line_field_less_json_returns_none() {
        // Falls through to the raw fallback at the parse_log_line level.
        assert!(parse_saved_line(r#"{"level":"info"}"#, 1).is_none());
    }

    #[test]
    fn saved_line_garbage_returns_none() {
        assert!(parse_saved_line("not a log line at all", 1).is_none());
    }

    // --- parse_iso_line ---

    #[test]
    fn parse_iso_line_with_service() {
        let entry = parse_iso_line("2024-03-01T08:00:00Z host nginx[42]: request handled", 1).unwrap();
        assert_eq!(entry.timestamp, "2024-03-01 08:00:00Z");
        assert_eq!(entry.service, "nginx");
        assert_eq!(entry.message, "request handled");
    }

    #[test]
    fn parse_iso_line_with_offset_and_fraction() {
        let entry = parse_iso_line("2024-03-01T08:00:00.500+02:00 plain message", 1).unwrap();
        assert_eq!(entry.timestamp, "2024-03-01 08:00:00.500+02:00");
        assert!(entry.service.is_empty());
        assert_eq!(entry.message, "plain message");
    }

    #[test]
    fn parse_iso_line_plain_message() {
        let entry = parse_iso_line("2024-03-01 08:00:00 just a plain message here", 1).unwrap();
        assert_eq!(entry.timestamp, "2024-03-01 08:00:00");
        assert!(entry.service.is_empty());
        assert_eq!(entry.message, "just a plain message here");
    }

    #[test]
    fn parse_iso_line_rejects_non_iso() {
        assert!(parse_iso_line("not a timestamped line", 1).is_none());
        assert!(parse_iso_line("May 29 10:30:45 host sshd: hi", 1).is_none());
    }

    // --- parse_raw_line ---

    #[test]
    fn parse_raw_line_keeps_whole_line() {
        let entry = parse_raw_line("some arbitrary text", 7).unwrap();
        assert_eq!(entry.line_num, 7);
        assert_eq!(entry.message, "some arbitrary text");
        assert!(entry.timestamp.is_empty());
        assert!(entry.service.is_empty());
    }

    #[test]
    fn parse_raw_line_infers_priority() {
        // "failed" -> error (3)
        assert_eq!(parse_raw_line("the operation failed", 1).unwrap().priority, 3);
    }

    #[test]
    fn parse_raw_line_blank_returns_none() {
        assert!(parse_raw_line("", 1).is_none());
    }

    // --- journal_to_log_entry ---

    #[test]
    fn journal_to_log_entry_with_microseconds() {
        let e = JournalEntry {
            realtime_timestamp: Some("1700000000123456".to_string()),
            priority: Some("4".to_string()),
            syslog_identifier: Some("nginx".to_string()),
            systemd_unit: None,
            message: Some("warn msg".to_string()),
        };
        let entry = journal_to_log_entry(1, &e);
        assert_eq!(entry.service, "nginx");
        assert_eq!(entry.priority, 4);
        assert!(entry.timestamp.contains('.'), "should have microseconds: {}", entry.timestamp);
    }

    #[test]
    fn journal_to_log_entry_whole_seconds() {
        let e = JournalEntry {
            realtime_timestamp: Some("1700000000000000".to_string()),
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("connected".to_string()),
        };
        let entry = journal_to_log_entry(1, &e);
        assert!(!entry.timestamp.contains('.'), "should have no fractional: {}", entry.timestamp);
    }

    #[test]
    fn journal_to_log_entry_missing_timestamp() {
        let e = JournalEntry {
            realtime_timestamp: None,
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("hello".to_string()),
        };
        let entry = journal_to_log_entry(5, &e);
        assert!(entry.timestamp.is_empty());
        assert_eq!(entry.line_num, 5);
    }
