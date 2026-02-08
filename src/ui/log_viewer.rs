use eframe::egui;
use crate::analyzer::{LogStore, LogEntry, FilterCriteria};

fn priority_color(priority: u8) -> egui::Color32 {
    match priority {
        0..=2 => egui::Color32::from_rgb(255, 80, 80),   // crit/alert/emerg - red
        3 => egui::Color32::from_rgb(255, 100, 100),       // error - lighter red
        4 => egui::Color32::from_rgb(255, 200, 60),        // warning - yellow
        5 => egui::Color32::from_rgb(100, 180, 255),       // notice - blue
        6 => egui::Color32::from_rgb(200, 200, 200),       // info - gray
        _ => egui::Color32::from_rgb(140, 140, 140),       // debug - dim gray
    }
}

fn priority_label(priority: u8) -> &'static str {
    match priority {
        0 => "EMERG",
        1 => "ALERT",
        2 => "CRIT",
        3 => "ERR",
        4 => "WARN",
        5 => "NOTICE",
        6 => "INFO",
        7 => "DEBUG",
        _ => "???",
    }
}

pub struct LogViewer {
    pub auto_scroll: bool,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self { auto_scroll: true }
    }
}

impl LogViewer {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        store: &LogStore,
        filtered_indices: &[usize],
        filter: &FilterCriteria,
    ) {
        let row_height = 18.0;
        let total_rows = filtered_indices.len();

        if total_rows == 0 {
            ui.centered_and_justified(|ui| {
                ui.label("No log entries to display");
            });
            return;
        }

        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Header
                ui.horizontal(|ui| {
                    let widths = [60.0, 160.0, 60.0, 150.0];
                    ui.add_sized([widths[0], row_height], egui::Label::new(
                        egui::RichText::new("Line#").strong().monospace(),
                    ));
                    ui.add_sized([widths[1], row_height], egui::Label::new(
                        egui::RichText::new("Time").strong().monospace(),
                    ));
                    ui.add_sized([widths[2], row_height], egui::Label::new(
                        egui::RichText::new("Pri").strong().monospace(),
                    ));
                    ui.add_sized([widths[3], row_height], egui::Label::new(
                        egui::RichText::new("Service").strong().monospace(),
                    ));
                    ui.label(egui::RichText::new("Message").strong().monospace());
                });

                ui.separator();

                // Virtual-scrolling log table
                let mut scroll = egui::ScrollArea::vertical()
                    .auto_shrink([false, false]);

                if self.auto_scroll {
                    scroll = scroll.stick_to_bottom(true);
                }

                scroll.show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for row_idx in row_range {
                        let entry_idx = filtered_indices[row_idx];
                        let entry = &store.entries[entry_idx];
                        self.render_row(ui, entry, row_height, filter);
                    }
                });
            });
    }

    fn render_row(
        &self,
        ui: &mut egui::Ui,
        entry: &LogEntry,
        row_height: f32,
        filter: &FilterCriteria,
    ) {
        let pri_color = priority_color(entry.priority);
        let widths = [60.0, 160.0, 60.0, 150.0];

        ui.horizontal(|ui| {
            ui.add_sized([widths[0], row_height], egui::Label::new(
                egui::RichText::new(format!("{}", entry.line_num))
                    .monospace()
                    .color(egui::Color32::from_rgb(120, 120, 120)),
            ));

            ui.add_sized([widths[1], row_height], egui::Label::new(
                egui::RichText::new(&entry.timestamp)
                    .monospace()
                    .color(egui::Color32::from_rgb(180, 180, 180)),
            ));

            ui.add_sized([widths[2], row_height], egui::Label::new(
                egui::RichText::new(priority_label(entry.priority))
                    .monospace()
                    .color(pri_color),
            ));

            ui.add_sized([widths[3], row_height], egui::Label::new(
                egui::RichText::new(&entry.service)
                    .monospace()
                    .color(egui::Color32::from_rgb(130, 200, 255)),
            ));

            // Message with regex highlighting
            if let Some(ref regex) = filter.pattern {
                let msg = &entry.message;
                let mut job = egui::text::LayoutJob::default();
                let mut last_end = 0;

                for m in regex.find_iter(msg) {
                    // Text before match
                    if m.start() > last_end {
                        job.append(
                            &msg[last_end..m.start()],
                            0.0,
                            egui::TextFormat {
                                font_id: egui::FontId::monospace(13.0),
                                color: egui::Color32::from_rgb(220, 220, 220),
                                ..Default::default()
                            },
                        );
                    }
                    // Highlighted match
                    job.append(
                        m.as_str(),
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::monospace(13.0),
                            color: egui::Color32::BLACK,
                            background: egui::Color32::from_rgb(255, 180, 50),
                            ..Default::default()
                        },
                    );
                    last_end = m.end();
                }
                // Text after last match
                if last_end < msg.len() {
                    job.append(
                        &msg[last_end..],
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::monospace(13.0),
                            color: egui::Color32::from_rgb(220, 220, 220),
                            ..Default::default()
                        },
                    );
                }

                ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
            } else {
                ui.add(egui::Label::new(
                    egui::RichText::new(&entry.message)
                        .monospace()
                        .color(egui::Color32::from_rgb(220, 220, 220)),
                ).wrap_mode(egui::TextWrapMode::Extend));
            }
        });
    }
}
