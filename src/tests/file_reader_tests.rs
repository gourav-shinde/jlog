    use super::*;

    // --- do_read integration ---
    // Per-format parsing unit tests live in parser_tests.rs (the shared parser).

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
                        this line has no recognizable format\n";
        let path = write_tmp("mixed", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        // The unrecognized line is now kept as a raw entry rather than dropped.
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].message, "this line has no recognizable format");
        assert!(entries[2].timestamp.is_empty());
        assert!(entries[2].service.is_empty());
    }

    #[test]
    fn do_read_plaintext_no_timestamp_no_service() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        // A free-form plaintext file with no timestamps or services at all.
        let content = "Starting up the application\n\
                        Something failed badly\n\
                        all done\n";
        let path = write_tmp("rawplain", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "Starting up the application");
        assert!(entries[0].timestamp.is_empty());
        assert!(entries[0].service.is_empty());
        // priority is inferred from content: "failed" -> error (3)
        assert_eq!(entries[1].priority, 3);
    }

    #[test]
    fn do_read_iso_file() {
        use crossbeam_channel::unbounded;
        use crate::background::BackgroundMessage;

        let content = "2024-01-15T10:30:45.123Z myhost sshd[1234]: Accepted publickey\n\
                        2024-01-15T10:30:46+02:00 application started successfully\n";
        let path = write_tmp("iso", content);
        let (tx, rx) = unbounded::<BackgroundMessage>();
        do_read(&path, &tx).unwrap();
        std::fs::remove_file(&path).ok();

        let (entries, completed) = drain_channel(&rx);
        assert!(completed);
        assert_eq!(entries.len(), 2);
        // 'T' separator normalized to space; fraction + zone preserved.
        assert_eq!(entries[0].timestamp, "2024-01-15 10:30:45.123Z");
        assert_eq!(entries[0].service, "sshd");
        assert_eq!(entries[0].message, "Accepted publickey");
        // No service prefix -> whole remainder is the message.
        assert_eq!(entries[1].timestamp, "2024-01-15 10:30:46+02:00");
        assert!(entries[1].service.is_empty());
        assert_eq!(entries[1].message, "application started successfully");
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
