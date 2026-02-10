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
    /// Index into LogStore.entries of the selected row, or None.
    pub selected_entry: Option<usize>,
    new_entry_count: usize,
    is_at_bottom: bool,
    /// Row index (in filtered list) to scroll to. Consumed after use.
    pub scroll_to_row: Option<usize>,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            auto_scroll: true,
            selected_entry: None,
            new_entry_count: 0,
            is_at_bottom: true,
            scroll_to_row: None,
        }
    }
}

/// Try to pretty-format a JSON string. Returns None if not valid JSON.
fn try_pretty_json(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

impl LogViewer {
    pub fn notify_new_entries(&mut self, count: usize) {
        if !self.auto_scroll && !self.is_at_bottom {
            self.new_entry_count += count;
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        store: &LogStore,
        filtered_indices: &[usize],
        filter: &FilterCriteria,
        find_pattern: Option<&regex::Regex>,
        current_find_row: Option<usize>,
    ) {
        let row_height = 18.0;
        let total_rows = filtered_indices.len();

        if total_rows == 0 {
            ui.centered_and_justified(|ui| {
                ui.label("No log entries to display");
            });
            return;
        }

        // Detail panel at the bottom when a row is selected
        if let Some(entry_idx) = self.selected_entry {
            if let Some(entry) = store.entries.get(entry_idx) {
                egui::TopBottomPanel::bottom("detail_panel")
                    .resizable(true)
                    .default_height(180.0)
                    .min_height(80.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("Row Detail");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("X Close").clicked() {
                                    self.selected_entry = None;
                                }
                            });
                        });
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("detail_grid")
                                    .num_columns(2)
                                    .spacing([10.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new("Line:").strong());
                                        ui.label(egui::RichText::new(format!("{}", entry.line_num)).monospace());
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Timestamp:").strong());
                                        ui.label(egui::RichText::new(&entry.timestamp).monospace());
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Priority:").strong());
                                        ui.label(egui::RichText::new(priority_label(entry.priority))
                                            .monospace()
                                            .color(priority_color(entry.priority)));
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Service:").strong());
                                        ui.label(egui::RichText::new(&entry.service)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(130, 200, 255)));
                                        ui.end_row();
                                    });

                                ui.separator();
                                ui.label(egui::RichText::new("Message:").strong());

                                if let Some(pretty) = try_pretty_json(&entry.message) {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&pretty)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(180, 230, 140)),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend)
                                        .selectable(true),
                                    );
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&entry.message)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(220, 220, 220)),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Wrap)
                                        .selectable(true),
                                    );
                                }
                            });
                    });
            }
        }

        // Log table
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

                let selected = self.selected_entry;
                let mut new_selection = self.selected_entry;

                // Handle scroll-to-row: set initial offset before building scroll area
                if let Some(target_row) = self.scroll_to_row.take() {
                    let target_offset = target_row as f32 * row_height;
                    scroll = scroll.vertical_scroll_offset(target_offset);
                    // Disable auto-scroll when user navigates via find
                    self.auto_scroll = false;
                }

                let scroll_output = scroll.show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for row_idx in row_range {
                        let entry_idx = filtered_indices[row_idx];
                        let entry = &store.entries[entry_idx];
                        let is_selected = selected == Some(entry_idx);
                        let is_current_find = current_find_row == Some(row_idx);

                        let resp = self.render_row(ui, entry, row_height, filter, is_selected, find_pattern, is_current_find);
                        if resp.clicked() {
                            new_selection = if is_selected { None } else { Some(entry_idx) };
                        }
                    }
                });

                // Detect if scrolled to bottom
                let threshold = 5.0;
                let state = scroll_output.state;
                let content_height = total_rows as f32 * row_height;
                let viewport_height = scroll_output.inner_rect.height();
                self.is_at_bottom = state.offset.y + viewport_height >= content_height - threshold;

                if self.is_at_bottom || self.auto_scroll {
                    self.new_entry_count = 0;
                }

                // Render floating "N new entries" indicator
                if self.new_entry_count > 0 {
                    let button_width = 160.0;
                    let button_height = 28.0;
                    let outer_rect = scroll_output.inner_rect;
                    let button_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            outer_rect.center().x,
                            outer_rect.bottom() - button_height / 2.0 - 8.0,
                        ),
                        egui::vec2(button_width, button_height),
                    );

                    let label = format!("\u{2193} {} new entries", self.new_entry_count);
                    let button = egui::Button::new(
                        egui::RichText::new(label)
                            .color(egui::Color32::WHITE)
                            .strong(),
                    )
                    .fill(egui::Color32::from_rgb(30, 130, 160));

                    if ui.put(button_rect, button).clicked() {
                        self.auto_scroll = true;
                        self.new_entry_count = 0;
                    }
                }

                self.selected_entry = new_selection;
            });
    }

    fn render_row(
        &self,
        ui: &mut egui::Ui,
        entry: &LogEntry,
        row_height: f32,
        filter: &FilterCriteria,
        is_selected: bool,
        find_pattern: Option<&regex::Regex>,
        is_current_find: bool,
    ) -> egui::Response {
        let pri_color = priority_color(entry.priority);
        let widths = [60.0, 160.0, 60.0, 150.0];

        let row_resp = ui.horizontal(|ui| {
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

            // Message with regex highlighting (filter = orange, find = green)
            let has_filter = filter.pattern.is_some();
            let has_find = find_pattern.is_some();

            if has_filter || has_find {
                let msg = &entry.message;
                let mut job = egui::text::LayoutJob::default();

                // Collect all highlight spans: (start, end, is_find)
                let mut spans: Vec<(usize, usize, bool)> = Vec::new();
                if let Some(ref regex) = filter.pattern {
                    for m in regex.find_iter(msg) {
                        spans.push((m.start(), m.end(), false));
                    }
                }
                if let Some(find_re) = find_pattern {
                    for m in find_re.find_iter(msg) {
                        spans.push((m.start(), m.end(), true));
                    }
                }
                // Sort by start; find highlights take priority (rendered on top)
                spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)));

                // Merge overlapping spans, keeping track of highlight type
                // Simple approach: iterate char by char through highlight ranges
                let default_fmt = egui::TextFormat {
                    font_id: egui::FontId::monospace(13.0),
                    color: egui::Color32::from_rgb(220, 220, 220),
                    ..Default::default()
                };
                let filter_fmt = egui::TextFormat {
                    font_id: egui::FontId::monospace(13.0),
                    color: egui::Color32::BLACK,
                    background: egui::Color32::from_rgb(255, 180, 50),
                    ..Default::default()
                };
                let find_fmt = egui::TextFormat {
                    font_id: egui::FontId::monospace(13.0),
                    color: egui::Color32::BLACK,
                    background: egui::Color32::from_rgb(80, 200, 120),
                    ..Default::default()
                };

                // Build a per-byte highlight type: 0=none, 1=filter, 2=find
                let len = msg.len();
                let mut hl = vec![0u8; len];
                for &(start, end, is_find) in &spans {
                    for b in start..end.min(len) {
                        if is_find {
                            hl[b] = 2;
                        } else if hl[b] == 0 {
                            hl[b] = 1;
                        }
                    }
                }

                // Emit runs of same highlight type
                let mut i = 0;
                while i < len {
                    // Find char boundary
                    if !msg.is_char_boundary(i) { i += 1; continue; }
                    let kind = hl[i];
                    let run_start = i;
                    while i < len && hl[i] == kind {
                        i += 1;
                    }
                    // Snap to char boundary
                    while i < len && !msg.is_char_boundary(i) { i += 1; }
                    let fmt = match kind {
                        1 => &filter_fmt,
                        2 => &find_fmt,
                        _ => &default_fmt,
                    };
                    job.append(&msg[run_start..i], 0.0, fmt.clone());
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

        // Make the whole row rect clickable and paint background
        let rect = row_resp.response.rect;
        let response = ui.interact(rect, ui.id().with(entry.line_num), egui::Sense::click());

        if is_selected {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(40, 60, 90, 180));
        } else if is_current_find {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(30, 80, 50, 160));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(50, 50, 65, 120));
        }

        response
    }
}
