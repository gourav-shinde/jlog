    use super::*;

    // --- SshConfig::default ---

    #[test]
    fn ssh_config_default_has_port_22() {
        let cfg = SshConfig::default();
        assert_eq!(cfg.port, 22);
        assert!(cfg.host.is_empty());
        assert!(cfg.username.is_empty());
        assert!(!cfg.command.is_empty());
        assert!(matches!(cfg.auth, AuthMethod::Agent));
    }

    // --- parse_ssh_line ---

    #[test]
    fn parse_ssh_line_valid_json() {
        let json = r#"{"__REALTIME_TIMESTAMP":"1700000000000000","PRIORITY":"6","SYSLOG_IDENTIFIER":"sshd","MESSAGE":"accepted"}"#;
        let e = parse_ssh_line(json, &mut 0).unwrap();
        assert_eq!(e.service(), "sshd");
        assert_eq!(e.msg(), "accepted");
    }

    #[test]
    fn parse_ssh_line_syslog_format() {
        let line = "May 29 10:30:45 host sshd[22]: Accepted publickey";
        let e = parse_ssh_line(line, &mut 0).unwrap();
        assert_eq!(e.service(), "sshd");
    }

    #[test]
    fn parse_ssh_line_garbage_returns_none_and_increments_errors() {
        let mut errs = 0usize;
        assert!(parse_ssh_line("not a log line", &mut errs).is_none());
        assert_eq!(errs, 1);
    }

    #[test]
    fn parse_ssh_line_empty_returns_none() {
        let mut errs = 0usize;
        // empty string doesn't start with '{' and fails syslog parse
        assert!(parse_ssh_line("", &mut errs).is_none());
        assert_eq!(errs, 1);
    }

    // --- journal_to_log_entry ---

    #[test]
    fn journal_to_log_entry_with_microseconds() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: Some("1700000000123456".to_string()),
            priority: Some("4".to_string()),
            syslog_identifier: Some("nginx".to_string()),
            systemd_unit: None,
            message: Some("warn".to_string()),
        };
        let log = journal_to_log_entry(1, &entry);
        assert_eq!(log.service, "nginx");
        assert_eq!(log.priority, 4);
        assert!(log.timestamp.contains('.'), "expected microseconds: {}", log.timestamp);
    }

    #[test]
    fn journal_to_log_entry_whole_seconds() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: Some("1700000000000000".to_string()),
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("ok".to_string()),
        };
        let log = journal_to_log_entry(2, &entry);
        assert!(!log.timestamp.contains('.'), "unexpected microseconds: {}", log.timestamp);
    }

    #[test]
    fn journal_to_log_entry_no_timestamp() {
        let entry = crate::journalctl::JournalEntry {
            realtime_timestamp: None,
            priority: Some("6".to_string()),
            syslog_identifier: Some("sshd".to_string()),
            systemd_unit: None,
            message: Some("msg".to_string()),
        };
        let log = journal_to_log_entry(3, &entry);
        assert!(log.timestamp.is_empty());
        assert_eq!(log.line_num, 3);
    }

    // --- AuthMethod constructors ---

    #[test]
    fn auth_method_variants_can_be_constructed() {
        let _pw = AuthMethod::Password("secret".to_string());
        let _kf = AuthMethod::KeyFile(std::path::PathBuf::from("/home/user/.ssh/id_rsa"));
        let _ag = AuthMethod::Agent;
    }

    // --- SshConfig clone ---

    #[test]
    fn ssh_config_can_be_cloned() {
        let cfg = SshConfig {
            host: "myhost".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
            command: "journalctl -f".to_string(),
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.host, "myhost");
        assert_eq!(cloned.port, 2222);
    }
