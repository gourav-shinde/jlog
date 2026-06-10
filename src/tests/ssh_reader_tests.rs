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

    // Log-line parsing is covered by the shared parser_tests.rs.

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
