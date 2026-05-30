    use super::*;
    use crate::analyzer::FilterCriteria;

    // --- priority_max ---

    #[test]
    fn priority_max_all_levels() {
        assert_eq!(priority_max(0), 7); // All
        assert_eq!(priority_max(1), 6); // INFO+
        assert_eq!(priority_max(2), 5); // NOTICE+
        assert_eq!(priority_max(3), 4); // WARN+
        assert_eq!(priority_max(4), 3); // ERR+
        assert_eq!(priority_max(5), 2); // CRIT+
        assert_eq!(priority_max(99), 7); // out-of-range defaults to all
    }

    // --- FilterBar::is_active ---

    #[test]
    fn default_bar_is_not_active() {
        assert!(!FilterBar::default().is_active());
    }

    #[test]
    fn is_active_when_pattern_set() {
        let mut bar = FilterBar::default();
        bar.pattern_text = "error".to_string();
        assert!(bar.is_active());
    }

    #[test]
    fn is_active_when_pattern2_set() {
        let mut bar = FilterBar::default();
        bar.pattern2_text = "warn".to_string();
        assert!(bar.is_active());
    }

    #[test]
    fn is_active_when_service_selected() {
        let mut bar = FilterBar::default();
        bar.selected_services.insert("sshd".to_string());
        assert!(bar.is_active());
    }

    #[test]
    fn is_active_when_priority_non_default() {
        let mut bar = FilterBar::default();
        bar.priority_choice = 3;
        assert!(bar.is_active());
    }

    #[test]
    fn is_active_when_combine_mode_changed() {
        let mut bar = FilterBar::default();
        bar.combine_mode = CombineMode::And;
        assert!(bar.is_active());
    }

    // --- FilterBar::apply_to_filter ---

    #[test]
    fn apply_to_filter_default_bar_resets_filter() {
        let bar = FilterBar::default();
        let mut filter = FilterCriteria::default();
        filter.max_priority = 3;
        bar.apply_to_filter(&mut filter);
        assert_eq!(filter.max_priority, 7);
        assert!(filter.units.is_empty());
        assert!(filter.pattern.is_none());
        assert!(filter.pattern2.is_none());
    }

    #[test]
    fn apply_to_filter_sets_pattern() {
        let mut bar = FilterBar::default();
        bar.pattern_text = "error".to_string();
        let mut filter = FilterCriteria::default();
        bar.apply_to_filter(&mut filter);
        assert!(filter.pattern.is_some());
    }

    #[test]
    fn apply_to_filter_sets_priority() {
        let mut bar = FilterBar::default();
        bar.priority_choice = 4; // ERR+
        let mut filter = FilterCriteria::default();
        bar.apply_to_filter(&mut filter);
        assert_eq!(filter.max_priority, 3);
    }

    #[test]
    fn apply_to_filter_sets_services() {
        let mut bar = FilterBar::default();
        bar.selected_services.insert("sshd".to_string());
        let mut filter = FilterCriteria::default();
        bar.apply_to_filter(&mut filter);
        assert!(filter.units.contains("sshd"));
    }

    #[test]
    fn apply_to_filter_sets_combine_mode() {
        let mut bar = FilterBar::default();
        bar.combine_mode = CombineMode::Or;
        let mut filter = FilterCriteria::default();
        bar.apply_to_filter(&mut filter);
        assert_eq!(filter.combine_mode, CombineMode::Or);
    }

    #[test]
    fn apply_to_filter_empty_pattern_leaves_none() {
        let bar = FilterBar::default(); // empty pattern
        let mut filter = FilterCriteria::default();
        bar.apply_to_filter(&mut filter);
        assert!(filter.pattern.is_none());
        assert!(filter.pattern2.is_none());
    }

    // --- headless egui: show() ---

    fn run_ui(mut f: impl FnMut(&egui::Context)) {
        let ctx = egui::Context::default();
        ctx.run(egui::RawInput::default(), |c| f(c));
    }

    #[test]
    fn show_default_bar_renders_without_panic() {
        let mut bar = FilterBar::default();
        let mut filter = FilterCriteria::default();
        let services = vec!["sshd".to_string(), "kernel".to_string()];

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &services, &mut filter);
            });
        });
    }

    #[test]
    fn show_with_and_mode_renders_second_pattern_field() {
        let mut bar = FilterBar::default();
        bar.combine_mode = CombineMode::And;
        bar.pattern_text = "error".to_string();
        bar.pattern2_text = "ssh".to_string();
        let mut filter = FilterCriteria::default();
        let services = vec![];

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &services, &mut filter);
            });
        });
    }

    #[test]
    fn show_with_or_mode_renders_second_pattern_field() {
        let mut bar = FilterBar::default();
        bar.combine_mode = CombineMode::Or;
        let mut filter = FilterCriteria::default();
        let services = vec![];

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &services, &mut filter);
            });
        });
    }

    #[test]
    fn show_with_selected_services_renders_without_panic() {
        let mut bar = FilterBar::default();
        bar.selected_services.insert("sshd".to_string());
        let mut filter = FilterCriteria::default();
        let services = vec!["sshd".to_string(), "nginx".to_string()];

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &services, &mut filter);
            });
        });
    }

    #[test]
    fn show_with_invalid_pattern_renders_without_panic() {
        let mut bar = FilterBar::default();
        bar.pattern_text = "[bad".to_string();
        bar.pattern_valid = false;
        let mut filter = FilterCriteria::default();

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &[], &mut filter);
            });
        });
    }

    #[test]
    fn show_with_priority_set_renders_without_panic() {
        let mut bar = FilterBar::default();
        bar.priority_choice = 3; // WARN+
        let mut filter = FilterCriteria::default();

        run_ui(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bar.show(ui, &[], &mut filter);
            });
        });
    }
