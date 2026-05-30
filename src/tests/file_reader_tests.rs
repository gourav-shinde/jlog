    use super::*;
    use crate::journalctl::JournalEntry;

    // --- do_read integration ---

    fn write_tmp(label: &str, content: &str) -> String {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};
        let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let path = format!("/tmp/jlog_fr_test_{}_{}.log", label, ns);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn drain_channel(rx: &crossbeam_channel::Receiver<crate::background::BackgroundMessage>)
        -> (Vec<crate::analyzer::LogEntry>, bool)
    {
        use crate::background::BackgroundMessage;
        let mut entries = vec![];
        let mut completed = false;
        loop {
            match rx.try_recv() {
                Ok(BackgroundMessage::Entry(e)) => entries.push(e),
                Ok(BackgroundMessage::Completed { .. }) => { completed = true; break; }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        (entries, completed)
    }

    #[test]
    fn do_read_plaintext_file() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        let content = "2026-01-01 10:00:00 sshd[6]: Connected\n\
                        2026-01-01 10:00:01 kernel[3]: Error occurred\n";
        let path = write_tmp("plain", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].service, "sshd");
        assert_eq!(entries[1].service, "kernel");
    }

    #[test]
    fn do_read_json_file() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        let content = "{\"__REALTIME_TIMESTAMP\":\"1700000000000000\",\"PRIORITY\":\"6\",\"SYSLOG_IDENTIFIER\":\"nginx\",\"MESSAGE\":\"started\"}\n";
        let path = write_tmp("json", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service, "nginx");
    }

    #[test]
    fn do_read_mixed_file() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        let content = "2026-01-01 10:00:00 sshd[6]: Connected\n\
                        {\"__REALTIME_TIMESTAMP\":\"1700000000000000\",\"PRIORITY\":\"3\",\"SYSLOG_IDENTIFIER\":\"kernel\",\"MESSAGE\":\"oops\"}\n\
                        this is garbage and should be skipped\n";
        let path = write_tmp("mixed", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn do_read_empty_file() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        let path = write_tmp("empty", "");
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert!(entries.is_empty());
    }

    #[test]
    fn do_read_saved_plaintext_with_microseconds() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        // Saved plaintext format with microsecond timestamp (the bug fix case)
        let content = "2026-01-01 10:00:00.123456 sshd[6]: Connected\n\
                        2026-01-01 10:00:01.000000 kernel[3]: Error\n";
        let path = write_tmp("savedus", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].service, "sshd");
        assert_eq!(entries[0].timestamp, "2026-01-01 10:00:00.123456");
        assert_eq!(entries[1].service, "kernel");
    }

    // --- parse_line ---

    #[test]
    fn parse_line_valid_json_journal_entry() {
        let json = r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","SYSLOG_IDENTIFIER":"sshd","MESSAGE":"hello"}"#;
        let entry = parse_line(json, &mut 0).unwrap();
        assert_eq!(entry.service(), "sshd");
        assert_eq!(entry.msg(), "hello");
    }

    #[test]
    fn parse_line_syslog_format() {
        let line = "May 29 10:30:45 host sshd[1234]: Accepted publickey";
        let entry = parse_line(line, &mut 0).unwrap();
        assert_eq!(entry.service(), "sshd");
        assert_eq!(entry.msg(), "Accepted publickey");
    }

    #[test]
    fn parse_line_empty_returns_none() {
        assert!(parse_line("", &mut 0).is_none());
        assert!(parse_line("   ", &mut 0).is_none());
    }

    #[test]
    fn parse_line_garbage_returns_none() {
        assert!(parse_line("not a log line", &mut 0).is_none());
    }

    #[test]
    fn parse_line_invalid_json_returns_none() {
        assert!(parse_line("{not valid json}", &mut 0).is_none());
    }

    // --- journal_to_log_entry with microseconds ---

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
        // timestamp evenly divisible by 1_000_000 → no fractional part
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

    // --- plaintext parsing ---

    #[test]
    fn plaintext_seconds_precision() {
        let entry = parse_saved_line("2026-02-11 10:30:45 sshd[6]: Connected", 1).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45");
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.priority, 6);
        assert_eq!(entry.message, "Connected");
    }

    #[test]
    fn plaintext_microsecond_precision() {
        let entry = parse_saved_line("2026-02-11 10:30:45.123456 sshd[6]: Connected", 1).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45.123456");
        assert_eq!(entry.service, "sshd");
        assert_eq!(entry.priority, 6);
        assert_eq!(entry.message, "Connected");
    }

    #[test]
    fn plaintext_empty_message() {
        let entry = parse_saved_line("2026-02-11 10:30:45.000001 kernel[3]: ", 2).unwrap();
        assert_eq!(entry.timestamp, "2026-02-11 10:30:45.000001");
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
    fn garbage_line_returns_none() {
        assert!(parse_saved_line("not a log line at all", 1).is_none());
        assert!(parse_saved_line("", 1).is_none());
    }
