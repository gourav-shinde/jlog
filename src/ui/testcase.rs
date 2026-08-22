use eframe::egui;

/// A user-defined test case tagging a span of log entries recorded between
/// "Start" and "Stop". Entries are referenced by their index into
/// `LogStore.entries`, which is stable as new entries stream in.
#[derive(Clone, Debug)]
pub struct TestCase {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Index of the first captured entry (entries.len() at Start).
    pub start_entry: usize,
    /// Index one past the last captured entry (entries.len() at Stop).
    /// `None` while the test case is still recording.
    pub end_entry: Option<usize>,
}

impl TestCase {
    /// The half-open range `[start, end)` of captured entry indices, clamping
    /// the (possibly still-open) upper bound to `total` entries.
    pub fn range(&self, total: usize) -> std::ops::Range<usize> {
        let start = self.start_entry.min(total);
        let end = self.end_entry.unwrap_or(total).min(total).max(start);
        start..end
    }

    pub fn is_recording(&self) -> bool {
        self.end_entry.is_none()
    }
}

/// Modal dialog collecting the id/name/description for a new test case.
#[derive(Default)]
pub struct TestCaseDialog {
    pub open: bool,
    id: String,
    name: String,
    description: String,
}

impl TestCaseDialog {
    /// Reset the fields and open the dialog.
    pub fn open(&mut self) {
        self.id.clear();
        self.name.clear();
        self.description.clear();
        self.open = true;
    }

    /// Returns `(id, name, description)` when the user confirms.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<(String, String, String)> {
        if !self.open {
            return None;
        }

        let mut result = None;
        let mut should_close = false;

        egui::Window::new("Start Test Case")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(420.0);

                egui::Grid::new("test_case_fields")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("ID:");
                        ui.text_edit_singleline(&mut self.id);
                        ui.end_row();

                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.name);
                        ui.end_row();

                        ui.label("Description:");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.description)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    let can_start = !self.id.trim().is_empty();
                    if ui
                        .add_enabled(can_start, egui::Button::new("Start Recording"))
                        .clicked()
                    {
                        result = Some((
                            self.id.trim().to_string(),
                            self.name.trim().to_string(),
                            self.description.trim().to_string(),
                        ));
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                    if !can_start {
                        ui.label(
                            egui::RichText::new("An ID is required")
                                .small()
                                .color(egui::Color32::from_rgb(200, 160, 60)),
                        );
                    }
                });
            });

        if should_close {
            self.open = false;
        }

        result
    }
}

#[cfg(test)]
#[path = "../tests/testcase_tests.rs"]
mod tests;
