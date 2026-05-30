use std::collections::HashSet;
use eframe::egui;
use crate::analyzer::{FilterCriteria, CombineMode};

#[derive(Clone)]
pub struct FilterBar {
    pub pattern_text: String,
    pub pattern2_text: String,
    pub pattern_valid: bool,
    pub pattern2_valid: bool,
    pub selected_services: HashSet<String>,
    pub service_search: String,   // autocomplete input text
    pub service_highlight: Option<usize>, // keyboard-nav index in dropdown (0 = "All")
    pub priority_choice: usize,   // index into PRIORITY_LABELS
    pub combine_mode: CombineMode,
}

const PRIORITY_LABELS: &[&str] = &[
    "All (debug+)",
    "INFO+",
    "NOTICE+",
    "WARN+",
    "ERR+",
    "CRIT+",
];

fn priority_max(choice: usize) -> u8 {
    match choice {
        0 => 7, // all
        1 => 6, // info+
        2 => 5, // notice+
        3 => 4, // warn+
        4 => 3, // err+
        5 => 2, // crit+
        _ => 7,
    }
}

const QUICK_PATTERNS: &[(&str, &str)] = &[
    ("Errors", "(?i)(error|fail|fatal)"),
    ("Warnings", "(?i)(warn|timeout|denied)"),
    ("SSH", "(?i)(ssh|sshd|auth)"),
    ("Kernel", "(?i)(kernel|oom|segfault)"),
    ("Systemd", "(?i)(systemd|service|unit)"),
];

impl Default for FilterBar {
    fn default() -> Self {
        Self {
            pattern_text: String::new(),
            pattern2_text: String::new(),
            pattern_valid: true,
            pattern2_valid: true,
            selected_services: HashSet::new(),
            service_search: String::new(),
            service_highlight: None,
            priority_choice: 0,
            combine_mode: CombineMode::Match,
        }
    }
}

impl FilterBar {
    /// Returns true if any filter is set (non-default state).
    pub fn is_active(&self) -> bool {
        !self.pattern_text.is_empty()
            || !self.pattern2_text.is_empty()
            || !self.selected_services.is_empty()
            || self.priority_choice != 0
            || self.combine_mode != CombineMode::Match
    }

    /// Reconstruct a FilterCriteria from the bar's current state.
    pub fn apply_to_filter(&self, filter: &mut FilterCriteria) {
        *filter = FilterCriteria::default();
        if !self.pattern_text.is_empty() {
            filter.set_pattern(&self.pattern_text);
        }
        if !self.pattern2_text.is_empty() {
            filter.set_pattern2(&self.pattern2_text);
        }
        filter.units = self.selected_services.clone();
        filter.max_priority = priority_max(self.priority_choice);
        filter.combine_mode = self.combine_mode;
    }

    /// Show filter bar UI. Returns true if filter changed.
    pub fn show(&mut self, ui: &mut egui::Ui, services: &[String], filter: &mut FilterCriteria) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Regex:");
            let color = if self.pattern_valid { egui::Color32::WHITE } else { egui::Color32::RED };
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.pattern_text)
                    .desired_width(200.0)
                    .text_color(color)
                    .hint_text("filter pattern..."),
            );
            if resp.changed() {
                self.pattern_valid = filter.set_pattern(&self.pattern_text);
                if self.pattern_valid {
                    changed = true;
                }
            }

            // Combine mode buttons
            ui.separator();
            for mode in &[CombineMode::Match, CombineMode::And, CombineMode::Or, CombineMode::Not] {
                let label = match mode {
                    CombineMode::Match => "Match",
                    CombineMode::And => "AND",
                    CombineMode::Or => "OR",
                    CombineMode::Not => "NOT",
                };
                if ui.selectable_label(self.combine_mode == *mode, label).clicked() {
                    self.combine_mode = *mode;
                    filter.combine_mode = *mode;
                    changed = true;
                }
            }

            // Second regex for AND/OR modes
            if self.combine_mode == CombineMode::And || self.combine_mode == CombineMode::Or {
                let color2 = if self.pattern2_valid { egui::Color32::WHITE } else { egui::Color32::RED };
                let resp2 = ui.add(
                    egui::TextEdit::singleline(&mut self.pattern2_text)
                        .desired_width(150.0)
                        .text_color(color2)
                        .hint_text("second pattern..."),
                );
                if resp2.changed() {
                    self.pattern2_valid = filter.set_pattern2(&self.pattern2_text);
                    if self.pattern2_valid {
                        changed = true;
                    }
                }
            }

            ui.separator();

            // Service filter with autocomplete
            ui.label("Service:");

            let hint = if self.selected_services.is_empty() {
                "all services...".to_string()
            } else {
                format!("{} selected", self.selected_services.len())
            };

            let svc_resp = ui.add(
                egui::TextEdit::singleline(&mut self.service_search)
                    .desired_width(160.0)
                    .hint_text(hint),
            );

            // Compute matches here so key handling and popup share the same list.
            let search_lower = self.service_search.to_lowercase();
            let matches: Vec<&String> = services
                .iter()
                .filter(|s| search_lower.is_empty() || s.to_lowercase().contains(&search_lower))
                .collect();

            // +1 accounts for the "All" row at index 0.
            let total = matches.len() + 1;

            let popup_id = ui.make_persistent_id("svc_autocomplete");

            if svc_resp.gained_focus() || svc_resp.changed() {
                ui.memory_mut(|m| m.open_popup(popup_id));
            }
            if svc_resp.changed() {
                self.service_highlight = None;
            }

            // Clamp highlight if matches shrunk.
            if let Some(h) = self.service_highlight {
                if h >= total {
                    self.service_highlight = Some(total.saturating_sub(1));
                }
            }

            // Arrow-key navigation when the text box has focus.
            let popup_open = ui.memory(|m| m.is_popup_open(popup_id));
            if svc_resp.has_focus() || popup_open {
                let (key_down, key_up, key_enter) = ui.input_mut(|i| (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                ));

                if key_down || key_up {
                    ui.memory_mut(|m| m.open_popup(popup_id));
                }
                if key_down {
                    self.service_highlight = Some(
                        self.service_highlight.map(|h| (h + 1) % total).unwrap_or(0),
                    );
                }
                if key_up {
                    self.service_highlight = Some(
                        self.service_highlight
                            .map(|h| if h == 0 { total - 1 } else { h - 1 })
                            .unwrap_or(total.saturating_sub(1)),
                    );
                }
                if key_enter {
                    if let Some(idx) = self.service_highlight {
                        if idx == 0 {
                            self.selected_services.clear();
                            filter.units.clear();
                            self.service_search.clear();
                            changed = true;
                        } else if let Some(svc) = matches.get(idx - 1) {
                            if self.selected_services.contains(*svc) {
                                self.selected_services.remove(*svc);
                            } else {
                                self.selected_services.insert((*svc).clone());
                            }
                            filter.units = self.selected_services.clone();
                            changed = true;
                        }
                    }
                }
            }

            egui::popup_below_widget(
                ui,
                popup_id,
                &svc_resp,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(220.0);
                    let highlight_color = ui.visuals().selection.bg_fill.linear_multiply(0.4);
                    let row_h = ui.spacing().interact_size.y;

                    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                        // "All" row
                        let all_highlighted = self.service_highlight == Some(0);
                        if all_highlighted {
                            let r = ui.available_rect_before_wrap();
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(r.min, egui::vec2(r.width(), row_h)),
                                0.0,
                                highlight_color,
                            );
                        }
                        let all_resp = ui.selectable_label(self.selected_services.is_empty(), "All");
                        if all_resp.clicked() {
                            self.selected_services.clear();
                            filter.units.clear();
                            self.service_search.clear();
                            changed = true;
                        }
                        if all_highlighted {
                            all_resp.scroll_to_me(Some(egui::Align::Center));
                        }

                        for (i, svc) in matches.iter().enumerate() {
                            let idx = i + 1;
                            let is_highlighted = self.service_highlight == Some(idx);
                            if is_highlighted {
                                let r = ui.available_rect_before_wrap();
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(r.min, egui::vec2(r.width(), row_h)),
                                    0.0,
                                    highlight_color,
                                );
                            }
                            let mut is_selected = self.selected_services.contains(*svc);
                            let resp = ui.checkbox(&mut is_selected, svc.as_str());
                            if resp.changed() {
                                if is_selected {
                                    self.selected_services.insert((*svc).clone());
                                } else {
                                    self.selected_services.remove(*svc);
                                }
                                filter.units = self.selected_services.clone();
                                changed = true;
                            }
                            if is_highlighted {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
                },
            );

            // Priority combo
            ui.label("Priority:");
            egui::ComboBox::from_id_salt("priority_filter")
                .selected_text(PRIORITY_LABELS[self.priority_choice])
                .show_ui(ui, |ui| {
                    for (i, label) in PRIORITY_LABELS.iter().enumerate() {
                        if ui.selectable_label(self.priority_choice == i, *label).clicked() {
                            self.priority_choice = i;
                            filter.max_priority = priority_max(i);
                            changed = true;
                        }
                    }
                });
        });

        // Quick pattern buttons row
        ui.horizontal(|ui| {
            ui.label("Quick:");
            for (label, pat) in QUICK_PATTERNS {
                if ui.small_button(*label).clicked() {
                    self.pattern_text = pat.to_string();
                    self.pattern_valid = filter.set_pattern(&self.pattern_text);
                    changed = true;
                }
            }
            if ui.small_button("Clear").clicked() {
                self.pattern_text.clear();
                self.pattern2_text.clear();
                self.selected_services.clear();
                self.service_search.clear();
                self.priority_choice = 0;
                self.combine_mode = CombineMode::Match;
                *filter = FilterCriteria::default();
                self.pattern_valid = true;
                self.pattern2_valid = true;
                changed = true;
            }
        });

        changed
    }
}

#[cfg(test)]
mod tests {
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
}
