use crate::tui::app::{App, Dialog, InputFocus, Operation, StatusState};
use crate::tui::helpers::{
    device_label, device_list_entry, device_path, file_basename, file_section_label, format_size,
};
use crate::tui::layout::{
    centered_rect, compute_layout, main_card_constraints, MIN_COLS, MIN_ROWS, PANEL_WIDTH_FULL,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::path::Path;

// Palette aligned with lithographer HTML UI (zinc + blue/cyan accents).
const BG: Color = Color::Rgb(9, 9, 11);
const CARD_BG: Color = Color::Rgb(15, 23, 42);
const BORDER: Color = Color::Rgb(39, 39, 42);
const TEXT: Color = Color::Rgb(228, 228, 231);
const MUTED: Color = Color::Rgb(113, 113, 122);
const ACCENT: Color = Color::Rgb(96, 165, 250);
const CYAN: Color = Color::Rgb(34, 211, 238);
const EMERALD: Color = Color::Rgb(52, 211, 153);
const AMBER: Color = Color::Rgb(251, 191, 36);
const ORANGE: Color = Color::Rgb(251, 146, 60);
const RED: Color = Color::Rgb(248, 113, 113);

pub fn ui(f: &mut Frame, app: &App) {
    f.render_widget(Clear, f.area());
    f.render_widget(Block::default().style(Style::default().bg(BG)), f.area());

    let layout = compute_layout(f.area());

    if layout.too_small {
        render_terminal_too_small(f, f.area());
        return;
    }

    render_header(f, app, layout.header);
    render_main_card(f, app, layout.main_card, layout.compact);
    render_footer(f, app, layout.footer, layout.show_shortcut_hints);

    match app.dialog {
        Dialog::None => {}
        Dialog::NonRemovableConfirm { target_index } => {
            render_confirmation_dialog(f, app, target_index);
        }
        Dialog::ElevationConfirm => render_elevation_dialog(f),
    }
}

fn render_terminal_too_small(f: &mut Frame, area: Rect) {
    let popup = centered_rect(52, 7, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Minimum size: {} columns × {} rows", MIN_COLS, MIN_ROWS),
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "Resize the terminal window to continue.",
            Style::default().fg(TEXT),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(AMBER))
        .style(Style::default().bg(CARD_BG))
        .title(" Resize required ");

    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1)])
        .split(area);

    let (status_label, status_color) = if app.is_root {
        ("● root", EMERALD)
    } else {
        ("● unprivileged", AMBER)
    };

    let mut title_spans = vec![
        Span::styled(
            "lithographer",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("     "),
        Span::styled(status_label, Style::default().fg(status_color)),
    ];

    if !app.is_root {
        let (polkit_label, polkit_color) = if app.polkit_available {
            ("polkit ready", EMERALD)
        } else {
            ("no polkit agent", RED)
        };
        title_spans.push(Span::raw("     "));
        title_spans.push(Span::styled(
            polkit_label,
            Style::default().fg(polkit_color),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(title_spans)).alignment(Alignment::Left),
        rows[0],
    );
    f.render_widget(
        Paragraph::new("SD Card • NVMe • USB Writer").style(Style::default().fg(MUTED)),
        rows[1],
    );
}

fn render_main_card(f: &mut Frame, app: &App, area: Rect, compact: bool) {
    let card_area = card_block("").inner(area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(CARD_BG)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .spacing(1)
        .constraints(main_card_constraints(compact))
        .split(card_area);

    f.render_widget(section_label("OPERATION MODE"), chunks[0]);
    render_mode_cards(f, app, chunks[1]);

    f.render_widget(section_label("TARGET DEVICE"), chunks[2]);
    render_device_select(f, app, chunks[3]);
    render_device_info(f, app, chunks[4]);

    f.render_widget(section_label(file_section_label(app.operation)), chunks[5]);
    render_file_select(f, app, chunks[6]);

    if app.operation == Operation::Flash {
        f.render_widget(section_label("VERIFY"), chunks[7]);
        render_verify_option(f, app, chunks[8]);
    }

    let status_chunk = if app.operation == Operation::Flash { 9 } else { 7 };
    let progress_label_chunk = status_chunk + 1;
    let progress_chunk = status_chunk + 2;
    let controls_chunk = status_chunk + 3;

    render_status(f, app, chunks[status_chunk]);

    f.render_widget(section_label("PROGRESS"), chunks[progress_label_chunk]);
    render_progress(f, app, chunks[progress_chunk]);

    render_controls(f, app, chunks[controls_chunk]);
}

fn render_verify_option(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == InputFocus::Verify;
    let border = if focused { CYAN } else { BORDER };
    let marker = if app.verify_checksum { "[x]" } else { "[ ]" };
    let label = format!("{marker} Verify checksum after write");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(CARD_BG));

    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(label).style(Style::default().fg(if app.is_running {
            MUTED
        } else {
            Color::White
        })),
        inner,
    );
}

fn render_mode_cards(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(1)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let flash_active = app.operation == Operation::Flash;
    let clone_active = app.operation == Operation::Clone;
    let mode_focused = app.focus == InputFocus::Mode;

    render_mode_card(
        f,
        cols[0],
        "Flash Image",
        "Write image to device",
        flash_active,
        mode_focused,
        "1/←",
    );
    render_mode_card(
        f,
        cols[1],
        "Clone Disk",
        "Create image from device",
        clone_active,
        mode_focused,
        "2/→",
    );
}

fn render_mode_card(
    f: &mut Frame,
    area: Rect,
    title: &str,
    desc: &str,
    active: bool,
    section_focused: bool,
    hint: &str,
) {
    let border_color = if active {
        ACCENT
    } else if section_focused {
        BORDER
    } else {
        BORDER
    };

    let bg = if active {
        Color::Rgb(15, 23, 42)
    } else {
        Color::Rgb(24, 24, 27)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg))
        .title(format!(" {} ", title))
        .title_style(if active {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled(desc, Style::default().fg(MUTED))),
        Line::from(Span::styled(
            hint,
            Style::default().fg(Color::Rgb(63, 63, 70)),
        )),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

fn render_device_select(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == InputFocus::Device;
    let border_color = if focused { ACCENT } else { BORDER };

    let label = if let Some(device) = app.selected_device() {
        device_label(device)
    } else if app.devices.is_empty() {
        "No storage devices found".to_string()
    } else {
        "Select a storage device...".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(24, 24, 27)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let line = Line::from(vec![
        Span::styled(label, focus_style(focused)),
        Span::raw("  "),
        Span::styled("▼", Style::default().fg(MUTED)),
    ]);
    f.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), inner);
}

fn render_device_info(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();

    if let Some(device) = app.selected_device() {
        let path = device_path(device);
        let removable = if device.removable == 1 {
            Span::styled(" ● Removable", Style::default().fg(EMERALD))
        } else {
            Span::styled(" ● Fixed", Style::default().fg(AMBER))
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", path), Style::default().fg(MUTED)),
            Span::styled(format_size(device.size), Style::default().fg(MUTED)),
            removable,
        ]));
    }

    if app.focus == InputFocus::Device {
        lines.push(Line::from(Span::styled(
            "Enter or d to choose device",
            Style::default().fg(Color::Rgb(63, 63, 70)),
        )));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_file_select(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == InputFocus::File;
    let border_color = if focused { ACCENT } else { BORDER };

    let (name, path_hint) = if app.image_file.is_empty() {
        match app.operation {
            Operation::Flash => (
                "No file selected".to_string(),
                "Enter/f to choose image".to_string(),
            ),
            Operation::Clone => (
                "No output file set".to_string(),
                "Enter/f to choose save location and name".to_string(),
            ),
        }
    } else {
        (file_basename(&app.image_file), app.image_file.clone())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(24, 24, 27)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("📄 ", Style::default().fg(ACCENT)),
            Span::styled(name, focus_style(focused)),
        ]),
        Line::from(Span::styled(path_hint, Style::default().fg(MUTED))),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(Color::Rgb(24, 24, 27)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Line::from(vec![
        Span::styled("STATUS", Style::default().fg(MUTED)),
        Span::raw("     "),
        Span::styled(
            status_label(app.status_state),
            Style::default()
                .fg(status_color(app.status_state))
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let detail = Line::from(Span::styled(
        app.status_detail.as_str(),
        Style::default().fg(Color::Rgb(161, 161, 170)),
    ));

    f.render_widget(
        Paragraph::new(vec![header, detail]).wrap(Wrap { trim: true }),
        inner,
    );
}

fn render_progress(f: &mut Frame, app: &App, area: Rect) {
    let pct = app.progress.clamp(0.0, 100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(39, 39, 42)))
        .ratio(pct as f64 / 100.0)
        .label(format!("{}%", pct));

    f.render_widget(gauge, area);
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let cols = if app.is_running {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(area)
    };

    let start_label = match app.operation {
        Operation::Flash => "▶  START FLASH",
        Operation::Clone => "▶  START CLONE",
    };

    let start_focused = app.focus == InputFocus::Start;
    let start_border = if start_focused { CYAN } else { ACCENT };
    let start_style = if app.is_running {
        Style::default().fg(MUTED)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    let start_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(start_border))
        .style(Style::default().bg(Color::Rgb(37, 99, 235)));

    let start_inner = start_block.inner(cols[0]);
    f.render_widget(start_block, cols[0]);
    f.render_widget(
        Paragraph::new(start_label)
            .style(start_style)
            .alignment(Alignment::Center),
        start_inner,
    );

    if app.is_running {
        let cancel_focused = app.focus == InputFocus::Cancel;
        let cancel_border = if cancel_focused {
            RED
        } else {
            Color::Rgb(127, 29, 29)
        };
        let cancel_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(cancel_border))
            .style(Style::default().bg(Color::Rgb(24, 24, 27)));

        let cancel_inner = cancel_block.inner(cols[1]);
        f.render_widget(cancel_block, cols[1]);
        f.render_widget(
            Paragraph::new("CANCEL")
                .style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center),
            cancel_inner,
        );
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect, show_shortcut_hints: bool) {
    let privilege_text = if app.is_root {
        "Privileged"
    } else {
        "Administrator privileges required for flash/clone"
    };

    let mut spans = vec![
        Span::styled("Made with ♥ by Girish Joshi", Style::default().fg(MUTED)),
        Span::raw("   │   "),
        Span::styled(privilege_text, Style::default().fg(MUTED)),
    ];

    if show_shortcut_hints {
        spans.push(Span::raw("   │   "));
        spans.push(Span::styled(
            "Tab · q quit · Enter start",
            Style::default().fg(MUTED),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

pub fn render_device_picker_dialog(f: &mut Frame, app: &App, list_index: usize) {
    let dialog_width = PANEL_WIDTH_FULL
        .saturating_sub(4)
        .max(40)
        .min(f.area().width.saturating_sub(4));

    let max_list_rows = f.area().height.saturating_sub(8).max(3) as usize;
    let estimated_visible = app.devices.len().clamp(1, max_list_rows);
    let dialog_height = (estimated_visible as u16 + 5).min(f.area().height.saturating_sub(2));

    let area = centered_rect(dialog_width, dialog_height, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(CARD_BG))
        .title(format!(
            " Select target device ({}/{}) ",
            list_index + 1,
            app.devices.len()
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let visible_count = chunks[0].height.max(1) as usize;
    let scroll = if list_index >= visible_count {
        list_index - visible_count + 1
    } else {
        0
    };

    let items: Vec<ListItem> = app
        .devices
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_count)
        .map(|(i, device)| device_list_entry(device, i == list_index))
        .collect();

    let list = List::new(items).style(Style::default().bg(CARD_BG));
    f.render_widget(list, chunks[0]);

    f.render_widget(
        Paragraph::new("↑↓ navigate · Enter select · r refresh · Esc cancel")
            .style(Style::default().fg(MUTED))
            .alignment(Alignment::Center),
        chunks[1],
    );
}

pub fn render_file_picker_hint(f: &mut Frame, operation: Operation) {
    let hint = match operation {
        Operation::Flash => "Select an existing image · Enter: confirm file · Esc: cancel",
        Operation::Clone => {
            "Navigate to output folder · Enter/n: name new file · →: open folder · Esc: cancel"
        }
    };

    let area = Rect {
        x: 1,
        y: f.area().height.saturating_sub(2),
        width: f.area().width.saturating_sub(2),
        height: 1,
    };

    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(MUTED))
            .alignment(Alignment::Center),
        area,
    );
}

pub fn render_output_filename_dialog(
    f: &mut Frame,
    directory: &Path,
    filename: &str,
    error: Option<&str>,
) {
    let area = centered_rect(62, 9, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(CARD_BG))
        .title(" Name output file ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut text = vec![
        Line::from(vec![
            Span::styled("Save to: ", Style::default().fg(MUTED)),
            Span::styled(directory.display().to_string(), Style::default().fg(TEXT)),
        ]),
        Line::from(Span::styled(
            "File will be created when cloning starts.",
            Style::default().fg(MUTED),
        )),
        Line::from(vec![
            Span::styled("Filename: ", Style::default().fg(MUTED)),
            Span::styled(
                filename,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(CYAN)),
        ]),
        Line::from(Span::styled(
            "Enter: confirm · Esc: cancel",
            Style::default().fg(MUTED),
        )),
    ];

    if let Some(err) = error {
        text.push(Line::from(Span::styled(err, Style::default().fg(RED))));
    }

    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

fn render_confirmation_dialog(f: &mut Frame, app: &App, target_index: usize) {
    let area = centered_rect(56, 7, f.area());
    f.render_widget(Clear, area);

    let device_name = app
        .devices
        .get(target_index)
        .map(|d| device_label(d))
        .unwrap_or_else(|| "unknown device".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(AMBER))
        .style(Style::default().bg(CARD_BG))
        .title(" Confirm device selection ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled(
            "This device is not removable. Writing to it can destroy data.",
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(device_name, Style::default().fg(AMBER))),
        Line::from(Span::styled(
            "Continue?  [Y] yes   [N]/Esc no",
            Style::default().fg(MUTED),
        )),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

pub fn render_elevation_dialog(f: &mut Frame) {
    let area = centered_rect(58, 9, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(CARD_BG))
        .title(" Administrator privileges required ");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled(
            "Flash and clone operations require root access.",
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            "You will be prompted for your password in a system dialog.",
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(
            "This window will close and a new elevated session will start.",
            Style::default().fg(MUTED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Continue?  [Y] yes   [N]/Esc no",
            Style::default().fg(MUTED),
        )),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

fn focus_style(active: bool) -> Style {
    if active {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    }
}

fn section_label(text: &str) -> Paragraph<'_> {
    Paragraph::new(text).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD))
}

fn card_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(CARD_BG))
        .title(title)
        .title_style(Style::default().fg(MUTED))
}

fn status_color(state: StatusState) -> Color {
    match state {
        StatusState::Ready => EMERALD,
        StatusState::InProgress => AMBER,
        StatusState::Complete => EMERALD,
        StatusState::Cancelled => ORANGE,
        StatusState::Error => RED,
    }
}

fn status_label(state: StatusState) -> &'static str {
    match state {
        StatusState::Ready => "Ready",
        StatusState::InProgress => "IN PROGRESS",
        StatusState::Complete => "COMPLETE",
        StatusState::Cancelled => "CANCELLED",
        StatusState::Error => "ERROR",
    }
}
