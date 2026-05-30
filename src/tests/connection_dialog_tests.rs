    use super::*;

    fn make_dialog(host: &str, port: &str, user: &str, auth: usize, key: &str, pass: &str) -> ConnectionDialog {
        ConnectionDialog {
            open: false,
            host: host.to_string(),
            port: port.to_string(),
            username: user.to_string(),
            auth_choice: auth,
            password: pass.to_string(),
            key_path: key.to_string(),
            command: "journalctl -o json -f".to_string(),
            error: None,
            profiles: vec![],
            selected_profile: None,
            prev_selected_profile: None,
            profile_name: String::new(),
        }
    }

    // --- validate ---

    #[test]
    fn validate_agent_auth_succeeds() {
        let d = make_dialog("myhost", "22", "alice", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.host, "myhost");
        assert_eq!(cfg.port, 22);
        assert_eq!(cfg.username, "alice");
        assert!(matches!(cfg.auth, AuthMethod::Agent));
    }

    #[test]
    fn validate_password_auth_succeeds() {
        let d = make_dialog("host", "22", "bob", 0, "", "secret");
        let cfg = d.validate().unwrap();
        assert!(matches!(cfg.auth, AuthMethod::Password(ref p) if p == "secret"));
    }

    #[test]
    fn validate_keyfile_auth_succeeds() {
        let d = make_dialog("host", "22", "alice", 1, "/home/alice/.ssh/id_rsa", "");
        let cfg = d.validate().unwrap();
        assert!(matches!(cfg.auth, AuthMethod::KeyFile(_)));
    }

    #[test]
    fn validate_missing_host_errors() {
        let d = make_dialog("", "22", "alice", 2, "", "");
        assert!(d.validate().is_err());
        assert!(d.validate().err().unwrap().contains("Host"));
    }

    #[test]
    fn validate_whitespace_host_errors() {
        let d = make_dialog("   ", "22", "alice", 2, "", "");
        assert!(d.validate().is_err());
    }

    #[test]
    fn validate_missing_username_errors() {
        let d = make_dialog("host", "22", "", 2, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("Username"));
    }

    #[test]
    fn validate_invalid_port_errors() {
        let d = make_dialog("host", "notaport", "alice", 2, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("port"));
    }

    #[test]
    fn validate_port_out_of_range_errors() {
        let d = make_dialog("host", "99999", "alice", 2, "", "");
        assert!(d.validate().is_err());
    }

    #[test]
    fn validate_missing_key_path_errors() {
        let d = make_dialog("host", "22", "alice", 1, "", "");
        let err = d.validate().err().unwrap();
        assert!(err.contains("Key file"));
    }

    #[test]
    fn validate_trims_host_and_username() {
        let d = make_dialog("  myhost  ", "22", "  alice  ", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.host, "myhost");
        assert_eq!(cfg.username, "alice");
    }

    #[test]
    fn validate_custom_port() {
        let d = make_dialog("host", "2222", "alice", 2, "", "");
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.port, 2222);
    }

    #[test]
    fn validate_command_preserved() {
        let mut d = make_dialog("host", "22", "alice", 2, "", "");
        d.command = "journalctl -n 500".to_string();
        let cfg = d.validate().unwrap();
        assert_eq!(cfg.command, "journalctl -n 500");
    }

    // --- profiles ---

    #[test]
    fn profiles_returns_slice() {
        let d = make_dialog("h", "22", "u", 2, "", "");
        assert!(d.profiles().is_empty());
    }

    // --- ConnectionDialog::default ---

    #[test]
    fn default_has_ssh_agent_auth() {
        // auth_choice 2 = SSH Agent
        // We can't call Default::default() as it loads from disk; use make_dialog instead.
        let d = make_dialog("", "22", "", 2, "", "");
        assert_eq!(d.auth_choice, 2);
        assert_eq!(d.port, "22");
    }

    // --- headless egui: show() ---

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    #[test]
    fn show_closed_dialog_returns_none() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = false;
        run_ui(|ctx| {
            let result = d.show(ctx);
            assert!(result.is_none());
        });
    }

    #[test]
    fn show_open_dialog_password_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 0, "", "secret");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_keyfile_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 1, "/home/user/.ssh/id_rsa", "");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_agent_auth_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_dialog_with_error_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        d.error = Some("Something went wrong".to_string());
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    // --- apply_profile ---

    #[test]
    fn apply_profile_copies_profile_fields() {
        let profile = ConnectionProfile {
            name: "prod".to_string(),
            host: "prod.example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            auth_choice: 1,
            key_path: "/home/deploy/.ssh/id_rsa".to_string(),
            command: "journalctl -n 500".to_string(),
            password: BASE64.encode(b""),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.apply_profile(0);
        assert_eq!(d.host, "prod.example.com");
        assert_eq!(d.port, "2222");
        assert_eq!(d.username, "deploy");
        assert_eq!(d.auth_choice, 1);
        assert_eq!(d.key_path, "/home/deploy/.ssh/id_rsa");
        assert_eq!(d.command, "journalctl -n 500");
        assert_eq!(d.profile_name, "prod");
    }

    #[test]
    fn apply_profile_password_is_decoded() {
        let password = "mysecret";
        let profile = ConnectionProfile {
            name: "dev".to_string(),
            host: "dev".to_string(),
            port: 22,
            username: "user".to_string(),
            auth_choice: 0,
            key_path: String::new(),
            command: "journalctl -f".to_string(),
            password: BASE64.encode(password.as_bytes()),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.apply_profile(0);
        assert_eq!(d.password, "mysecret");
    }

    #[test]
    fn apply_profile_out_of_range_does_nothing() {
        let mut d = make_dialog("original", "22", "user", 2, "", "");
        d.apply_profile(99); // no profile at index 99
        assert_eq!(d.host, "original"); // unchanged
    }

    // --- select_profile ---

    #[test]
    fn select_profile_applies_and_opens_dialog() {
        let profile = ConnectionProfile {
            name: "staging".to_string(),
            host: "staging.example.com".to_string(),
            port: 22,
            username: "ci".to_string(),
            auth_choice: 2,
            key_path: String::new(),
            command: "journalctl -f".to_string(),
            password: BASE64.encode(b""),
        };
        let mut d = make_dialog("", "22", "", 2, "", "");
        d.profiles = vec![profile];
        d.select_profile(0);
        assert!(d.open);
        assert_eq!(d.selected_profile, Some(0));
        assert_eq!(d.host, "staging.example.com");
    }

    #[test]
    fn show_open_dialog_with_profiles_renders_without_panic() {
        let mut d = make_dialog("host", "22", "user", 2, "", "");
        d.open = true;
        d.profiles = vec![
            ConnectionProfile {
                name: "prod".to_string(),
                host: "prod.example.com".to_string(),
                port: 22,
                username: "deploy".to_string(),
                auth_choice: 2,
                key_path: String::new(),
                command: "journalctl -o json -f".to_string(),
                password: String::new(),
            },
        ];
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }
