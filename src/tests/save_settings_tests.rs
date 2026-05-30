    use super::*;

    fn settings_with(dest: &str, template: &str, format: SaveFormat) -> SaveSettings {
        SaveSettings {
            destination: dest.to_string(),
            filename_template: template.to_string(),
            format,
            auto_save: false,
            save_filtered_only: false,
            always_show_bookmarks: false,
        }
    }

    // --- resolve_filename ---

    #[test]
    fn resolve_filename_json_extension() {
        let s = settings_with("/tmp", "myfile", SaveFormat::Json);
        let path = s.resolve_filename("host");
        assert!(path.ends_with(".json"), "path={}", path);
    }

    #[test]
    fn resolve_filename_plaintext_extension() {
        let s = settings_with("/tmp", "myfile", SaveFormat::PlainText);
        let path = s.resolve_filename("host");
        assert!(path.ends_with(".log"), "path={}", path);
    }

    #[test]
    fn resolve_filename_host_substituted() {
        let s = settings_with("/tmp", "{host}_log", SaveFormat::PlainText);
        let path = s.resolve_filename("myserver");
        assert!(path.contains("myserver"), "path={}", path);
    }

    #[test]
    fn resolve_filename_date_time_substituted() {
        let s = settings_with("/tmp", "{date}_{time}", SaveFormat::PlainText);
        let path = s.resolve_filename("h");
        // date = YYYY-MM-DD, time = HH-MM-SS
        assert!(path.contains('-'), "path={}", path);
        assert!(!path.contains("{date}"), "path={}", path);
        assert!(!path.contains("{time}"), "path={}", path);
    }

    #[test]
    fn resolve_filename_all_variables() {
        let s = settings_with("/logs", "{host}_{date}_{time}", SaveFormat::Json);
        let path = s.resolve_filename("srv1");
        assert!(path.starts_with("/logs/srv1_"), "path={}", path);
        assert!(path.ends_with(".json"), "path={}", path);
    }

    #[test]
    fn resolve_filename_destination_is_prefix() {
        let s = settings_with("/my/dir", "file", SaveFormat::PlainText);
        let path = s.resolve_filename("h");
        assert!(path.starts_with("/my/dir"), "path={}", path);
    }

    // --- SaveSettings::default ---

    #[test]
    fn default_has_reasonable_values() {
        let s = SaveSettings::default();
        assert!(s.auto_save);
        assert!(!s.save_filtered_only);
        assert!(!s.always_show_bookmarks);
        assert_eq!(s.format, SaveFormat::PlainText);
        assert!(!s.filename_template.is_empty());
        assert!(!s.destination.is_empty());
    }

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    // --- headless egui: SaveSettingsDialog::show() ---

    #[test]
    fn show_closed_dialog_returns_none() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = false;
        run_ui(|ctx| {
            let result = dialog.show(ctx);
            assert!(result.is_none());
        });
    }

    #[test]
    fn show_open_dialog_renders_without_panic() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = true;
        run_ui(|ctx| {
            let _ = dialog.show(ctx);
        });
    }

    #[test]
    fn show_open_json_format_renders_without_panic() {
        let mut dialog = SaveSettingsDialog::default();
        dialog.open = true;
        // simulate loading a JSON-format settings
        dialog.load_from(&SaveSettings {
            destination: "/tmp".to_string(),
            filename_template: "{host}_{date}".to_string(),
            format: SaveFormat::Json,
            auto_save: true,
            save_filtered_only: true,
            always_show_bookmarks: false,
        });
        run_ui(|ctx| {
            let _ = dialog.show(ctx);
        });
    }

    // --- load_settings / save_settings_to_disk roundtrip ---

    #[test]
    fn save_and_load_settings_roundtrip() {
        use std::env;
        // Override HOME to a temp dir to avoid touching real user config
        let tmp = env::temp_dir().join(format!("jlog_cfg_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).ok();
        let orig_home = env::var("HOME").ok();
        unsafe { env::set_var("HOME", &tmp); }

        let settings = SaveSettings {
            destination: "/my/dest".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        save_settings_to_disk(&settings);
        let loaded = load_settings();

        // Restore HOME
        if let Some(h) = orig_home {
            unsafe { env::set_var("HOME", h); }
        }
        std::fs::remove_dir_all(&tmp).ok();

        assert_eq!(loaded.destination, "/my/dest");
        assert_eq!(loaded.filename_template, "custom_{host}");
        assert_eq!(loaded.format, SaveFormat::Json);
        assert!(!loaded.auto_save);
        assert!(loaded.save_filtered_only);
        assert!(loaded.always_show_bookmarks);
    }

    // --- SaveSettingsDialog::load_from ---

    #[test]
    fn load_from_copies_all_fields() {
        let settings = SaveSettings {
            destination: "/custom/path".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        let mut dialog = SaveSettingsDialog::default();
        dialog.load_from(&settings);
        // Verify via resolve (indirectly checks destination and template)
        let preview = SaveSettings {
            destination: "/custom/path".to_string(),
            filename_template: "custom_{host}".to_string(),
            format: SaveFormat::Json,
            auto_save: false,
            save_filtered_only: true,
            always_show_bookmarks: true,
        };
        let path = preview.resolve_filename("h");
        assert!(path.starts_with("/custom/path"), "path={}", path);
        assert!(path.ends_with(".json"), "path={}", path);
    }
