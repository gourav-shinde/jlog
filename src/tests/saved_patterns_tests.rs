    use super::*;

    #[test]
    fn upsert_adds_new_pattern() {
        let mut sp = SavedPatterns::default();
        sp.upsert("errors", "(?i)error");
        assert_eq!(sp.patterns.len(), 1);
        assert_eq!(sp.patterns[0].name, "errors");
        assert_eq!(sp.patterns[0].pattern, "(?i)error");
    }

    #[test]
    fn upsert_replaces_existing_by_name() {
        let mut sp = SavedPatterns::default();
        sp.upsert("errors", "error");
        sp.upsert("errors", "(?i)error|fail");
        assert_eq!(sp.patterns.len(), 1);
        assert_eq!(sp.patterns[0].pattern, "(?i)error|fail");
    }

    #[test]
    fn upsert_trims_name_and_ignores_blank() {
        let mut sp = SavedPatterns::default();
        sp.upsert("  spaced  ", "x");
        sp.upsert("   ", "y");
        assert_eq!(sp.patterns.len(), 1);
        assert_eq!(sp.patterns[0].name, "spaced");
    }

    #[test]
    fn remove_deletes_by_name() {
        let mut sp = SavedPatterns::default();
        sp.upsert("a", "1");
        sp.upsert("b", "2");
        sp.remove("a");
        assert_eq!(sp.patterns.len(), 1);
        assert_eq!(sp.patterns[0].name, "b");
    }

    #[test]
    fn serde_round_trip_preserves_patterns() {
        let mut sp = SavedPatterns::default();
        sp.upsert("errors", "(?i)error");
        sp.upsert("ssh", "sshd");
        let json = serde_json::to_string(&sp).unwrap();
        let back: SavedPatterns = serde_json::from_str(&json).unwrap();
        assert_eq!(back.patterns, sp.patterns);
    }
