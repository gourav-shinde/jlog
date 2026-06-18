use eframe::egui;
use crate::analyzer::{LogStore, LogEntry, FilterCriteria};

pub fn priority_color(priority: u8) -> egui::Color32 {
    match priority {
        0..=2 => egui::Color32::from_rgb(255, 80, 80),   // crit/alert/emerg - red
        3 => egui::Color32::from_rgb(255, 100, 100),       // error - lighter red
        4 => egui::Color32::from_rgb(255, 200, 60),        // warning - yellow
        5 => egui::Color32::from_rgb(100, 180, 255),       // notice - blue
        6 => egui::Color32::from_rgb(200, 200, 200),       // info - gray
        _ => egui::Color32::from_rgb(140, 140, 140),       // debug - dim gray
    }
}

fn format_entry_for_copy(entry: &LogEntry) -> String {
    format!(
        "{} {}[{}]: {}",
        entry.timestamp,
        entry.service,
        priority_label(entry.priority),
        entry.message,
    )
}

pub fn priority_label(priority: u8) -> &'static str {
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
    /// Set to true when "Show in Context" is clicked; consumed by app.
    pub show_in_context_requested: bool,
    /// Entry index to toggle bookmark for; consumed by app.
    pub toggle_bookmark_requested: Option<usize>,
    /// Cached pretty-printed JSON for the selected entry: (entry_idx, pretty_or_none).
    cached_pretty: Option<(usize, Option<String>)>,
    /// Detail panel: show the raw message instead of pretty-printed JSON.
    detail_show_raw: bool,
    /// Detail panel: word-wrap the message instead of single-line + scroll.
    detail_wrap: bool,
}

impl Default for LogViewer {
    fn default() -> Self {
        Self {
            auto_scroll: true,
            selected_entry: None,
            new_entry_count: 0,
            is_at_bottom: true,
            scroll_to_row: None,
            show_in_context_requested: false,
            toggle_bookmark_requested: None,
            cached_pretty: None,
            detail_show_raw: false,
            detail_wrap: true,
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

/// Build a monospace `LayoutJob` for `text`, painting filter matches orange and
/// find matches green (find takes priority where they overlap). Used by the
/// detail panel; the row renderer has its own allocation-free variant.
fn build_highlight_job(
    text: &str,
    filter: Option<&regex::Regex>,
    find: Option<&regex::Regex>,
    base_color: egui::Color32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::monospace(13.0);

    let default_fmt = egui::TextFormat {
        font_id: font.clone(),
        color: base_color,
        ..Default::default()
    };

    if filter.is_none() && find.is_none() {
        job.append(text, 0.0, default_fmt);
        return job;
    }

    // Per-byte highlight kind: 0 = none, 1 = filter, 2 = find.
    let len = text.len();
    let mut hl = vec![0u8; len];
    if let Some(re) = filter {
        for m in re.find_iter(text) {
            for b in m.start()..m.end().min(len) {
                if hl[b] == 0 { hl[b] = 1; }
            }
        }
    }
    if let Some(re) = find {
        for m in re.find_iter(text) {
            for b in m.start()..m.end().min(len) {
                hl[b] = 2;
            }
        }
    }

    let filter_fmt = egui::TextFormat {
        font_id: font.clone(),
        color: egui::Color32::BLACK,
        background: egui::Color32::from_rgb(255, 180, 50),
        ..Default::default()
    };
    let find_fmt = egui::TextFormat {
        font_id: font.clone(),
        color: egui::Color32::BLACK,
        background: egui::Color32::from_rgb(80, 200, 120),
        ..Default::default()
    };

    // Emit runs of the same highlight kind, respecting char boundaries.
    let mut i = 0;
    while i < len {
        if !text.is_char_boundary(i) { i += 1; continue; }
        let kind = hl[i];
        let start = i;
        while i < len && hl[i] == kind { i += 1; }
        while i < len && !text.is_char_boundary(i) { i += 1; }
        let fmt = match kind {
            1 => &filter_fmt,
            2 => &find_fmt,
            _ => &default_fmt,
        };
        job.append(&text[start..i], 0.0, fmt.clone());
    }
    job
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
        show_context_button: bool,
        in_context_mode: bool,
        bookmarks: &std::collections::HashSet<usize>,
    ) {
        // Ctrl+C: copy selected entry to clipboard
        if ui.ctx().input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
            if let Some(idx) = self.selected_entry {
                if let Some(entry) = store.entries.get(idx) {
                    ui.ctx().copy_text(format_entry_for_copy(entry));
                }
            }
        }

        // B key: toggle bookmark on selected entry (only when no text widget is focused)
        let no_text_focus = !ui.ctx().memory(|m| m.focused().is_some());
        if no_text_focus && ui.ctx().input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::B)) {
            if let Some(idx) = self.selected_entry {
                self.toggle_bookmark_requested = Some(idx);
            }
        }

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
            // Recompute the JSON pretty-print only when the selection changes.
            match self.cached_pretty {
                Some((cached_idx, _)) if cached_idx == entry_idx => {}
                _ => {
                    self.cached_pretty = store.entries.get(entry_idx)
                        .map(|e| (entry_idx, try_pretty_json(&e.message)));
                }
            }
            // Clone the cached result so the closure below can own it without
            // holding a borrow on `self`.
            let pretty_msg: Option<String> = self.cached_pretty
                .as_ref()
                .and_then(|(_, p)| p.clone());

            if let Some(entry) = store.entries.get(entry_idx) {
                let is_json = pretty_msg.is_some();
                let is_bookmarked = bookmarks.contains(&entry_idx);
                // Position within the currently filtered ("shown") list.
                let shown_pos = filtered_indices.iter().position(|&i| i == entry_idx);
                let total_shown = filtered_indices.len();

                egui::TopBottomPanel::bottom("detail_panel")
                    .resizable(true)
                    .default_height(200.0)
                    .min_height(80.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("Row Detail");
                            if is_bookmarked {
                                ui.label(egui::RichText::new("\u{2605} bookmarked")
                                    .small()
                                    .color(egui::Color32::from_rgb(255, 200, 50)));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("\u{2715} Close").clicked() {
                                    self.selected_entry = None;
                                }
                                if show_context_button {
                                    if ui.small_button("Show in Context").clicked() {
                                        self.show_in_context_requested = true;
                                    }
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

                                        ui.label(egui::RichText::new("Position:").strong());
                                        let pos_text = match shown_pos {
                                            Some(p) => format!("row {} of {} shown", p + 1, total_shown),
                                            None => format!("{} shown", total_shown),
                                        };
                                        ui.label(egui::RichText::new(pos_text).monospace());
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Timestamp:").strong());
                                        let ts = if entry.timestamp.is_empty() { "\u{2014}" } else { entry.timestamp.as_str() };
                                        ui.label(egui::RichText::new(ts).monospace());
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Priority:").strong());
                                        ui.label(egui::RichText::new(priority_label(entry.priority))
                                            .monospace()
                                            .color(priority_color(entry.priority)));
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Service:").strong());
                                        let svc = if entry.service.is_empty() { "\u{2014}" } else { entry.service.as_str() };
                                        ui.label(egui::RichText::new(svc)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(130, 200, 255)));
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Length:").strong());
                                        ui.label(egui::RichText::new(format!(
                                            "{} chars",
                                            entry.message.chars().count()
                                        )).monospace());
                                        ui.end_row();
                                    });

                                ui.separator();

                                // Message header row: label + JSON badge + view toggles.
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Message:").strong());
                                    if is_json {
                                        ui.label(egui::RichText::new("JSON")
                                            .small()
                                            .strong()
                                            .color(egui::Color32::BLACK)
                                            .background_color(egui::Color32::from_rgb(120, 200, 140)));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.toggle_value(&mut self.detail_wrap, "Wrap");
                                        if is_json {
                                            // Two-state toggle: Pretty <-> Raw.
                                            if ui.selectable_label(!self.detail_show_raw, "Pretty").clicked() {
                                                self.detail_show_raw = false;
                                            }
                                            if ui.selectable_label(self.detail_show_raw, "Raw").clicked() {
                                                self.detail_show_raw = true;
                                            }
                                        }
                                    });
                                });

                                // Choose displayed text + base color.
                                let show_pretty = is_json && !self.detail_show_raw;
                                let (display_text, base_color): (&str, egui::Color32) = if show_pretty {
                                    (pretty_msg.as_deref().unwrap_or(&entry.message),
                                     egui::Color32::from_rgb(180, 230, 140))
                                } else {
                                    (entry.message.as_str(),
                                     egui::Color32::from_rgb(220, 220, 220))
                                };

                                // Build the message text with find/filter highlights applied.
                                let mut job = build_highlight_job(
                                    display_text,
                                    filter.pattern.as_ref(),
                                    find_pattern,
                                    base_color,
                                );

                                let frame = egui::Frame::group(ui.style())
                                    .fill(egui::Color32::from_rgb(24, 24, 28))
                                    .inner_margin(egui::Margin::same(6));

                                if self.detail_wrap {
                                    job.wrap.max_width = ui.available_width() - 24.0;
                                    job.wrap.break_anywhere = true;
                                    frame.show(ui, |ui| {
                                        ui.add(egui::Label::new(job).selectable(true));
                                    });
                                } else {
                                    job.wrap.max_width = f32::INFINITY;
                                    frame.show(ui, |ui| {
                                        egui::ScrollArea::horizontal()
                                            .id_salt("detail_msg_hscroll")
                                            .auto_shrink([false, true])
                                            .show(ui, |ui| {
                                                ui.add(egui::Label::new(job).selectable(true));
                                            });
                                    });
                                }
                            });
                    });
            }
        }

        // Log table.
        // Captured before entering the (horizontally scrolling) area, where
        // `available_width` would be unbounded. Used to keep the inner vertical
        // list at least viewport-wide so its scrollbar stays pinned to the
        // window's right edge instead of hugging the widest visible row.
        let viewport_width = ui.available_width();
        let vbar_allowance = ui.spacing().scroll.bar_width
            + ui.spacing().scroll.bar_inner_margin
            + ui.spacing().scroll.bar_outer_margin;
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

                // Virtual-scrolling log table.
                // Width shrinks to content (the widest visible row) instead of
                // filling the unbounded width offered by the outer horizontal
                // ScrollArea. Using `false` here creates a layout feedback loop
                // that makes the content jitter/resize while scrolling
                // horizontally.
                let mut scroll = egui::ScrollArea::vertical()
                    .auto_shrink([true, false]);

                // Handle scroll-to-row: set initial offset before building scroll area
                // Must be checked before stick_to_bottom to avoid conflict
                if let Some(target_row) = self.scroll_to_row.take() {
                    // show_rows uses (row_height + item_spacing.y) per row
                    let row_height_with_spacing = row_height + ui.spacing().item_spacing.y;
                    let target_offset = target_row as f32 * row_height_with_spacing;
                    scroll = scroll.vertical_scroll_offset(target_offset);
                    self.auto_scroll = false;
                }

                if self.auto_scroll {
                    scroll = scroll.stick_to_bottom(true);
                }

                let selected = self.selected_entry;
                let mut new_selection = self.selected_entry;
                let mut bookmark_toggle: Option<usize> = None;

                // Reused across rows to avoid per-row heap allocations.
                let mut spans_buf: Vec<(usize, usize, bool)> = Vec::new();
                let mut hl_buf: Vec<u8> = Vec::new();

                let scroll_output = scroll.show_rows(ui, row_height, total_rows, |ui, row_range| {
                    // Floor the content width to the viewport so the vertical
                    // scrollbar stays at the window edge even when every visible
                    // row is short. Wider rows still extend past this and scroll
                    // horizontally via the outer area.
                    ui.set_min_width((viewport_width - vbar_allowance).max(0.0));
                    for row_idx in row_range {
                        let entry_idx = filtered_indices[row_idx];
                        let entry = &store.entries[entry_idx];
                        let is_selected = selected == Some(entry_idx);
                        let is_current_find = current_find_row == Some(row_idx);
                        let is_bookmarked = bookmarks.contains(&entry_idx);

                        let is_context_highlight = in_context_mode && is_selected;
                        let resp = self.render_row(ui, entry, row_height, filter, is_selected, find_pattern, is_current_find, is_context_highlight, is_bookmarked, &mut spans_buf, &mut hl_buf);
                        if resp.clicked() {
                            new_selection = if is_selected { None } else { Some(entry_idx) };
                        }
                        let bookmark_label = if is_bookmarked { "Remove Bookmark" } else { "Bookmark" };
                        resp.context_menu(|ui| {
                            if ui.button("Copy Line").clicked() {
                                ui.ctx().copy_text(format_entry_for_copy(entry));
                                ui.close_menu();
                            }
                            if ui.button(bookmark_label).clicked() {
                                bookmark_toggle = Some(entry_idx);
                                ui.close_menu();
                            }
                        });
                    }
                });

                if let Some(idx) = bookmark_toggle {
                    self.toggle_bookmark_requested = Some(idx);
                }

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
        is_context_highlight: bool,
        is_bookmarked: bool,
        spans_buf: &mut Vec<(usize, usize, bool)>,
        hl_buf: &mut Vec<u8>,
    ) -> egui::Response {
        let pri_color = priority_color(entry.priority);
        let widths = [60.0, 160.0, 60.0, 150.0];

        let row_resp = ui.horizontal(|ui| {
            let (line_text, line_color) = if is_bookmarked {
                (format!("\u{2605}{}", entry.line_num), egui::Color32::from_rgb(255, 200, 50))
            } else {
                (format!("{}", entry.line_num), egui::Color32::from_rgb(120, 120, 120))
            };
            ui.add_sized([widths[0], row_height], egui::Label::new(
                egui::RichText::new(line_text)
                    .monospace()
                    .color(line_color),
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
                spans_buf.clear();
                if let Some(ref regex) = filter.pattern {
                    for m in regex.find_iter(msg) {
                        spans_buf.push((m.start(), m.end(), false));
                    }
                }
                if let Some(find_re) = find_pattern {
                    for m in find_re.find_iter(msg) {
                        spans_buf.push((m.start(), m.end(), true));
                    }
                }
                // Sort by start; find highlights take priority (rendered on top)
                spans_buf.sort_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)));

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
                hl_buf.clear();
                hl_buf.resize(len, 0);
                for &(start, end, is_find) in spans_buf.iter() {
                    for b in start..end.min(len) {
                        if is_find {
                            hl_buf[b] = 2;
                        } else if hl_buf[b] == 0 {
                            hl_buf[b] = 1;
                        }
                    }
                }

                // Emit runs of same highlight type
                let mut i = 0;
                while i < len {
                    if !msg.is_char_boundary(i) { i += 1; continue; }
                    let kind = hl_buf[i];
                    let run_start = i;
                    while i < len && hl_buf[i] == kind {
                        i += 1;
                    }
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

        if is_selected && is_context_highlight {
            // Bright highlight for "Show in Context" row
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(80, 130, 40, 200));
        } else if is_selected {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(40, 60, 90, 180));
        } else if is_current_find {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(30, 80, 50, 160));
        } else if is_bookmarked {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(80, 65, 10, 80));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(50, 50, 65, 120));
        }

        response
    }
}

#[cfg(test)]
#[path = "../tests/log_viewer_tests.rs"]
mod tests;
