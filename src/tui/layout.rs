use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 24;

pub const PANEL_WIDTH_FULL: u16 = 80;
pub const HEADER_HEIGHT: u16 = 5;
pub const FOOTER_HEIGHT: u16 = 2;

pub struct UiLayout {
    pub header: Rect,
    pub main_card: Rect,
    pub footer: Rect,
    pub too_small: bool,
    pub compact: bool,
}

pub fn terminal_too_small(area: Rect) -> bool {
    area.width < MIN_COLS || area.height < MIN_ROWS
}

pub fn compute_layout(area: Rect) -> UiLayout {
    if terminal_too_small(area) {
        return UiLayout {
            header: Rect::default(),
            main_card: Rect::default(),
            footer: Rect::default(),
            too_small: true,
            compact: true,
        };
    }

    let compact = area.width < PANEL_WIDTH_FULL || area.height < 40;
    let panel_width = area.width.saturating_sub(2).min(PANEL_WIDTH_FULL).max(MIN_COLS - 2);

    let main_card_height = if compact {
        area.height
            .saturating_sub(HEADER_HEIGHT + FOOTER_HEIGHT + 4)
            .max(12)
    } else {
        area.height
            .saturating_sub(HEADER_HEIGHT + FOOTER_HEIGHT + 4)
            .min(52)
            .max(20)
    };

    let panel_height = HEADER_HEIGHT + main_card_height + FOOTER_HEIGHT + 1;
    let top_pad = area.height.saturating_sub(panel_height) / 2;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(panel_height.min(area.height)),
            Constraint::Min(0),
        ])
        .split(area);

    let h_pad = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(panel_width.min(outer[1].width)),
            Constraint::Min(0),
        ])
        .split(outer[1]);

    let panel = h_pad[1];

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .spacing(1)
        .constraints([
            Constraint::Length(HEADER_HEIGHT.min(panel.height)),
            Constraint::Min(main_card_height.min(panel.height.saturating_sub(HEADER_HEIGHT + FOOTER_HEIGHT))),
            Constraint::Length(FOOTER_HEIGHT.min(panel.height)),
        ])
        .split(panel);

    UiLayout {
        header: sections[0],
        main_card: sections[1],
        footer: sections[2],
        too_small: false,
        compact,
    }
}

pub fn main_card_constraints(compact: bool) -> [Constraint; 11] {
    if compact {
        [
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(3),
        ]
    } else {
        [
            Constraint::Length(1),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(4),
        ]
    }
}

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}