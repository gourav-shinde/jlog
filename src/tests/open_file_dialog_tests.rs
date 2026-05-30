    use super::*;

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    #[test]
    fn default_is_closed() {
        let d = OpenFileDialog::default();
        assert!(!d.open);
        assert!(d.path.is_empty());
        assert!(d.error.is_none());
    }

    #[test]
    fn show_closed_returns_none() {
        let mut d = OpenFileDialog::default();
        d.open = false;
        run_ui(|ctx| {
            assert!(d.show(ctx).is_none());
        });
    }

    #[test]
    fn show_open_renders_without_panic() {
        let mut d = OpenFileDialog::default();
        d.open = true;
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_with_error_renders_without_panic() {
        let mut d = OpenFileDialog::default();
        d.open = true;
        d.error = Some("File not found: /bad/path".to_string());
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }

    #[test]
    fn show_open_with_path_renders_without_panic() {
        let mut d = OpenFileDialog::default();
        d.open = true;
        d.path = "/var/log/syslog".to_string();
        run_ui(|ctx| {
            let _ = d.show(ctx);
        });
    }
