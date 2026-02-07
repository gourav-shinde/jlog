use eframe::egui;
use crate::analyzer::{FilterCriteria, CombineMode};

pub struct FilterBar {
    pub pattern_text: String,
    pub pattern2_text: String,
    pub pattern_valid: bool,
    pub pattern2_valid: bool,
    pub selected_service: String, // "" = All
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
            selected_service: String::new(),
            priority_choice: 0,
            combine_mode: CombineMode::Match,
        }
    }
}

impl FilterBar {
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

            // Service combo
            ui.label("Service:");
            let current = if self.selected_service.is_empty() { "All" } else { &self.selected_service };
            egui::ComboBox::from_id_salt("service_filter")
                .selected_text(current)
                .width(150.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.selected_service.is_empty(), "All").clicked() {
                        self.selected_service.clear();
                        filter.unit = None;
                        changed = true;
                    }
                    for svc in services {
                        if ui.selectable_label(&self.selected_service == svc, svc).clicked() {
                            self.selected_service = svc.clone();
                            filter.unit = Some(svc.clone());
                            changed = true;
                        }
                    }
                });

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
                self.selected_service.clear();
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
