pub mod profiles;
pub mod save_settings;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};

use crate::app::{App, Mode};

// ─── Entry point ──────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.mode {
        Mode::Help      => { let a = f.area(); draw_main(f, app); draw_help(f, app, a); }
        Mode::Connect   => { let a = f.area(); draw_main(f, app); draw_connect(f, app, a); }
        Mode::Bookmarks => { let a = f.area(); draw_main(f, app); draw_bookmarks(f, app, a); }
        _               => draw_main(f, app),
    }
}

// ─── Main layout ──────────────────────────────────────────────────────────────

fn draw_main(f: &mut Frame, app: &mut App) {
    let show_input = matches!(app.mode, Mode::Filter | Mode::Find);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // title bar
            Constraint::Min(1),     // log table
            Constraint::Length(if show_input { 1 } else { 0 }), // input bar
            Constraint::Length(1),  // status bar
        ])
        .split(f.area());

    draw_title(f, app, chunks[0]);
    draw_log_table(f, app, chunks[1]);
    if show_input {
        draw_input_bar(f, app, chunks[2]);
    }
    draw_status(f, app, chunks[3]);
}

// ─── Title bar ────────────────────────────────────────────────────────────────

fn draw_title(f: &mut Frame, app: &App, area: Rect) {
    let conn_status = if app.is_connected {
        Span::styled(" ● LIVE ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else if app.is_loading {
        Span::styled(" ◌ LOADING ", Style::default().fg(Color::Yellow))
    } else if !app.current_host.is_empty() {
        Span::styled(" ○ OFFLINE ", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(" jlog ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    };

    let host = if app.current_host.is_empty() {
        Span::raw(" Log Viewer")
    } else {
        Span::styled(format!(" {}", app.current_host), Style::default().fg(Color::White))
    };

    let count = Span::styled(
        format!(" [{}/{}] ", app.filtered_indices.len(), app.log_store.entries.len()),
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![conn_status, host, count]);
    let title = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(30, 30, 50)));
    f.render_widget(title, area);
}

// ─── Log table ────────────────────────────────────────────────────────────────

fn priority_color(p: u8) -> Color {
    match p {
        0..=3 => Color::Red,
        4     => Color::Yellow,
        5     => Color::Cyan,
        6     => Color::White,
        _     => Color::DarkGray,
    }
}

fn priority_label(p: u8) -> &'static str {
    match p {
        0 => "EMRG", 1 => "ALRT", 2 => "CRIT", 3 => "ERR ",
        4 => "WARN", 5 => "NOTC", 6 => "INFO", _ => "DBUG",
    }
}

fn draw_log_table(f: &mut Frame, app: &mut App, area: Rect) {
    let visible = area.height.saturating_sub(2) as usize; // minus header + border
    app.ensure_selected_visible(visible);

    let start = app.scroll_offset;
    let end   = (start + visible).min(app.filtered_indices.len());

    let rows: Vec<Row> = (start..end).map(|row_idx| {
        let entry_idx = app.filtered_indices[row_idx];
        let entry = &app.log_store.entries[entry_idx];
        let is_selected = app.selected_row == Some(row_idx);
        let is_bookmarked = app.bookmarks.contains(&entry_idx);
        let is_find_match = app.find_match_set.contains(&row_idx);

        let bm_marker = if is_bookmarked { "★" } else { " " };

        let pri_span = Cell::from(priority_label(entry.priority))
            .style(Style::default().fg(priority_color(entry.priority)));

        let msg_style = if is_find_match {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let row = Row::new(vec![
            Cell::from(format!("{}{:>6}", bm_marker, entry.line_num))
                .style(Style::default().fg(if is_bookmarked { Color::Yellow } else { Color::DarkGray })),
            Cell::from(entry.timestamp.as_str())
                .style(Style::default().fg(Color::Rgb(150, 150, 150))),
            pri_span,
            Cell::from(entry.service.as_str())
                .style(Style::default().fg(Color::Cyan)),
            Cell::from(entry.message.as_str())
                .style(msg_style),
        ]);

        if is_selected {
            row.style(Style::default().bg(Color::Rgb(50, 50, 80)).add_modifier(Modifier::BOLD))
        } else {
            row
        }
    }).collect();

    let widths = [
        Constraint::Length(8),   // line# + bookmark
        Constraint::Length(20),  // timestamp
        Constraint::Length(5),   // priority
        Constraint::Length(20),  // service
        Constraint::Min(10),     // message
    ];

    let scroll_info = if app.filtered_indices.is_empty() {
        "empty".to_string()
    } else {
        format!("{}/{}", end, app.filtered_indices.len())
    };

    let filter_indicator = if !app.filter_text.is_empty() {
        format!(" [filter: {}]", app.filter_text)
    } else {
        String::new()
    };

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["  Line#", "Timestamp", "Pri", "Service", "Message"])
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(60, 60, 90)))
                .title(format!(" Logs{} ", filter_indicator))
                .title_alignment(Alignment::Left)
                .title_bottom(Line::from(format!(" {} ", scroll_info)).alignment(Alignment::Right))
        );

    let mut table_state = TableState::default();
    f.render_stateful_widget(table, area, &mut table_state);
}

// ─── Input bar (filter / find) ────────────────────────────────────────────────

fn draw_input_bar(f: &mut Frame, app: &App, area: Rect) {
    let (label, text, color) = match app.mode {
        Mode::Filter => ("Filter: ", app.filter_text.as_str(), Color::Green),
        Mode::Find   => ("Find:   ", app.find_text.as_str(), Color::Yellow),
        _ => return,
    };

    let find_info = if matches!(app.mode, Mode::Find) {
        if app.find_matches.is_empty() { "".to_string() }
        else { format!(" [{}/{}] n/N navigate", app.find_current + 1, app.find_matches.len()) }
    } else { String::new() };

    let line = Line::from(vec![
        Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::styled(text, Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(color)), // cursor
        Span::styled(find_info, Style::default().fg(Color::DarkGray)),
    ]);
    let bar = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(20, 20, 40)));
    f.render_widget(bar, area);
}

// ─── Status bar ───────────────────────────────────────────────────────────────

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let keys = match app.mode {
        Mode::Normal    => " q:quit  /:filter  ^F:find  c:connect  o:open  b:bookmark  B:bookmarks  r:reconnect  ?:help",
        Mode::Filter    => " Enter:apply  Esc:cancel  (prefix path with / to open file)",
        Mode::Find      => " Enter/n:next  N:prev  Esc:close",
        Mode::Connect   => " Tab:next field  Enter:action  Esc:cancel",
        Mode::Bookmarks => " j/k:navigate  Enter:jump  d:delete  c:clear  Esc:close",
        Mode::Help      => " Any key to close",
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(keys.len() as u16)])
        .split(area);

    let status = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(&app.status, Style::default().fg(Color::Rgb(180, 180, 180))),
    ])).style(Style::default().bg(Color::Rgb(20, 20, 40)));

    let hints = Paragraph::new(Line::from(
        Span::styled(keys, Style::default().fg(Color::Rgb(80, 80, 100)))
    )).style(Style::default().bg(Color::Rgb(20, 20, 40)));

    f.render_widget(status, chunks[0]);
    f.render_widget(hints, chunks[1]);
}

// ─── Connect dialog ───────────────────────────────────────────────────────────

fn draw_connect(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(60, 24, area);
    f.render_widget(Clear, popup);

    let auth_label = match app.conn.auth_choice {
        0 => "Password",
        1 => "Key File",
        _ => "SSH Agent",
    };

    let credential_label = match app.conn.auth_choice {
        0 => "Password",
        1 => "Key Path",
        _ => "Agent   ",
    };

    let credential_value = match app.conn.auth_choice {
        0 => app.conn.password.chars().map(|_| '*').collect::<String>(),
        1 => app.conn.key_path.clone(),
        _ => "(using SSH agent)".to_string(),
    };

    let profile_display = app.conn.profile_idx
        .and_then(|i| app.conn.profiles.get(i))
        .map(|p| p.name.as_str())
        .unwrap_or("(none)");

    let fields: &[(&str, &str, usize)] = &[
        ("Host    ", &app.conn.host,     0),
        ("Port    ", &app.conn.port,     1),
        ("Username", &app.conn.username, 2),
        ("Auth    ", auth_label,         3),
        (credential_label, &credential_value, 4),
        ("Command ", &app.conn.command,  5),
        ("Profile ", profile_display,    6),
        ("Prof.Name", &app.conn.profile_name, 7), // reusing field 9 -> rendered as 7th visual
    ];

    let mut lines: Vec<Line> = vec![Line::from("")];

    for (label, value, field_idx) in fields.iter() {
        let focused = app.conn.focused == *field_idx;
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        let value_style = if focused {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(200, 200, 200))
        };

        let cursor = if focused { "█" } else { "" };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{}: ", label), label_style),
            Span::styled(format!("{}{}", value, cursor), value_style),
        ]));
    }

    lines.push(Line::from(""));

    // Buttons
    let btn = |label: &'static str, idx: usize| {
        let focused = app.conn.focused == idx;
        if focused {
            Span::styled(format!("[{}]", label), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(format!("[{}]", label), Style::default().fg(Color::White))
        }
    };

    lines.push(Line::from(vec![
        Span::raw("  "),
        btn("Connect", 7),
        Span::raw("  "),
        btn("Save Profile", 8),
        Span::raw("  "),
        btn("Cancel", 9),
    ]));

    if !app.conn.error.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(&app.conn.error, Style::default().fg(Color::Red)),
        ]));
    }

    if !app.conn.profiles.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Profiles (↑↓ when Profile focused): ", Style::default().fg(Color::DarkGray)),
        ]));
        for (i, p) in app.conn.profiles.iter().enumerate() {
            let selected = app.conn.profile_idx == Some(i);
            let style = if selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(if selected { "▶ " } else { "  " }, style),
                Span::styled(&p.name, style),
            ]));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" SSH Connect ")
        .title_alignment(Alignment::Center);

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, popup);
}

// ─── Bookmarks overlay ────────────────────────────────────────────────────────

fn draw_bookmarks(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(80, 60, area);
    f.render_widget(Clear, popup);

    let mut sorted: Vec<usize> = app.bookmarks.iter().copied().collect();
    sorted.sort();

    let inner_height = popup.height.saturating_sub(2) as usize;
    let offset = if app.bookmark_selected >= inner_height {
        app.bookmark_selected + 1 - inner_height
    } else {
        0
    };

    let rows: Vec<Row> = sorted.iter().enumerate()
        .skip(offset)
        .take(inner_height)
        .map(|(i, &entry_idx)| {
            let entry = match app.log_store.entries.get(entry_idx) {
                Some(e) => e,
                None => return Row::new(vec![Cell::from("?")]),
            };
            let is_selected = i == app.bookmark_selected;
            let in_view = app.filtered_indices.contains(&entry_idx);

            let style = if is_selected {
                Style::default().bg(Color::Rgb(50, 50, 80)).add_modifier(Modifier::BOLD)
            } else if !in_view {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(format!("★{}", entry.line_num))
                    .style(Style::default().fg(Color::Yellow)),
                Cell::from(entry.timestamp.as_str())
                    .style(Style::default().fg(Color::Rgb(150, 150, 150))),
                Cell::from(priority_label(entry.priority))
                    .style(Style::default().fg(priority_color(entry.priority))),
                Cell::from(entry.service.as_str())
                    .style(Style::default().fg(Color::Cyan)),
                Cell::from(entry.message.as_str()),
            ]).style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Length(20),
        Constraint::Length(5),
        Constraint::Length(18),
        Constraint::Min(10),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(format!(" Bookmarks ({}) ", sorted.len()))
        .title_alignment(Alignment::Center)
        .title_bottom(Line::from(" Enter:jump  d:delete  c:clear  Esc:close ").alignment(Alignment::Center));

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["Line#", "Timestamp", "Pri", "Service", "Message"])
                .style(Style::default().add_modifier(Modifier::BOLD))
        )
        .block(block);

    let mut state = TableState::default();
    f.render_stateful_widget(table, popup, &mut state);
}

// ─── Help overlay ─────────────────────────────────────────────────────────────

fn draw_help(f: &mut Frame, _app: &App, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled("  Navigation", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from("  j / ↓       Move down"),
        Line::from("  k / ↑       Move up"),
        Line::from("  PgDn/PgUp   Scroll page"),
        Line::from("  g            Go to top"),
        Line::from("  G            Go to bottom"),
        Line::from(""),
        Line::from(vec![Span::styled("  Filtering & Search", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from("  /            Open filter bar (regex)"),
        Line::from("  Esc          Clear filter (in Normal mode)"),
        Line::from("  Ctrl+F       Find in current view"),
        Line::from("  n / N        Next / prev find match"),
        Line::from(""),
        Line::from(vec![Span::styled("  Bookmarks", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from("  b            Toggle bookmark on selection"),
        Line::from("  B            Open bookmarks view"),
        Line::from(""),
        Line::from(vec![Span::styled("  Connection", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from("  c            Open SSH connect dialog"),
        Line::from("  o            Open file (/ to filter, type path + Enter)"),
        Line::from("  r            Reconnect last SSH session"),
        Line::from("  d            Disconnect"),
        Line::from(""),
        Line::from(vec![Span::styled("  General", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from("  ?            Show this help"),
        Line::from("  q            Quit"),
        Line::from(""),
        Line::from(vec![Span::styled("  Press any key to close", Style::default().fg(Color::DarkGray))]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help — jlog ")
        .title_alignment(Alignment::Center);

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, popup);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}
