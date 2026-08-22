use std::collections::HashSet;
use eframe::egui;
use crate::analyzer::{FilterCriteria, CombineMode};
use crate::ui::saved_patterns::{SavedPatterns, save_patterns_to_disk};

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
    pub line_from_text: String,   // inclusive lower Line# bound (empty = unbounded)
    pub line_to_text: String,     // inclusive upper Line# bound (empty = unbounded)
    pub new_pattern_name: String, // transient input for the "save pattern" popup
}

/// Parse a Line# bound field: empty or non-numeric yields `None`.
fn parse_line_bound(text: &str) -> Option<usize> {
    let t = text.trim();
    if t.is_empty() { None } else { t.parse().ok() }
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
            line_from_text: String::new(),
            line_to_text: String::new(),
            new_pattern_name: String::new(),
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
            || !self.line_from_text.trim().is_empty()
            || !self.line_to_text.trim().is_empty()
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
        filter.min_line = parse_line_bound(&self.line_from_text);
        filter.max_line = parse_line_bound(&self.line_to_text);
    }

    /// Set the Line# range fields from a test case's captured span and push the
    /// change into `filter`. Returns without clearing other active filters.
    pub fn set_line_range(&mut self, from: usize, to: usize, filter: &mut FilterCriteria) {
        self.line_from_text = from.to_string();
        self.line_to_text = to.to_string();
        filter.min_line = Some(from);
        filter.max_line = Some(to);
    }

    /// Show filter bar UI. Returns true if filter changed.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        services: &[String],
        filter: &mut FilterCriteria,
        saved_patterns: &mut SavedPatterns,
    ) -> bool {
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

            // Save the current pattern under a name.
            let save_resp = ui.button("\u{2605} Save");
            let save_popup_id = ui.make_persistent_id("save_pattern_popup");
            if save_resp.clicked() {
                self.new_pattern_name.clear();
                ui.memory_mut(|m| m.toggle_popup(save_popup_id));
            }
            egui::popup_below_widget(
                ui,
                save_popup_id,
                &save_resp,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(220.0);
                    ui.label("Save pattern as:");
                    let name_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.new_pattern_name)
                            .hint_text("name..."),
                    );
                    let enter = name_resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let can_save = !self.new_pattern_name.trim().is_empty()
                        && !self.pattern_text.is_empty();
                    if (ui.button("Save").clicked() || enter) && can_save {
                        saved_patterns.upsert(&self.new_pattern_name, &self.pattern_text);
                        save_patterns_to_disk(saved_patterns);
                        self.new_pattern_name.clear();
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if self.pattern_text.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 160, 60),
                            "Enter a pattern first.",
                        );
                    }
                },
            );

            // Load / delete a saved pattern.
            let mut load_pattern: Option<String> = None;
            let mut delete_pattern: Option<String> = None;
            egui::ComboBox::from_id_salt("saved_patterns_combo")
                .selected_text("Saved \u{25be}")
                .show_ui(ui, |ui| {
                    if saved_patterns.patterns.is_empty() {
                        ui.label(
                            egui::RichText::new("(none saved)")
                                .italics()
                                .color(egui::Color32::from_rgb(140, 140, 140)),
                        );
                    }
                    for np in &saved_patterns.patterns {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, &np.name).clicked() {
                                load_pattern = Some(np.pattern.clone());
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("\u{2715}").clicked() {
                                    delete_pattern = Some(np.name.clone());
                                }
                            });
                        });
                    }
                });
            if let Some(pat) = load_pattern {
                self.pattern_text = pat;
                self.pattern_valid = filter.set_pattern(&self.pattern_text);
                changed = true;
            }
            if let Some(name) = delete_pattern {
                saved_patterns.remove(&name);
                save_patterns_to_disk(saved_patterns);
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
            ui.separator();

            // Line# range filter.
            ui.label("Line#");
            let from_resp = ui.add(
                egui::TextEdit::singleline(&mut self.line_from_text)
                    .desired_width(60.0)
                    .hint_text("from"),
            );
            ui.label("\u{2013}");
            let to_resp = ui.add(
                egui::TextEdit::singleline(&mut self.line_to_text)
                    .desired_width(60.0)
                    .hint_text("to"),
            );
            if from_resp.changed() || to_resp.changed() {
                filter.min_line = parse_line_bound(&self.line_from_text);
                filter.max_line = parse_line_bound(&self.line_to_text);
                changed = true;
            }

            if ui.small_button("Clear").clicked() {
                self.pattern_text.clear();
                self.pattern2_text.clear();
                self.selected_services.clear();
                self.service_search.clear();
                self.priority_choice = 0;
                self.combine_mode = CombineMode::Match;
                self.line_from_text.clear();
                self.line_to_text.clear();
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
#[path = "../tests/filter_bar_tests.rs"]
mod tests;
