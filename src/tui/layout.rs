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
    pub show_shortcut_hints: bool,
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
            show_shortcut_hints: false,
        };
    }

    let compact = area.width < PANEL_WIDTH_FULL || area.height < 40;
    let show_shortcut_hints = !compact && area.height >= 30;
    let panel_width = area
        .width
        .saturating_sub(2)
        .min(PANEL_WIDTH_FULL)
        .max(MIN_COLS - 2);

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
            Constraint::Min(
                main_card_height.min(panel.height.saturating_sub(HEADER_HEIGHT + FOOTER_HEIGHT)),
            ),
            Constraint::Length(FOOTER_HEIGHT.min(panel.height)),
        ])
        .split(panel);

    UiLayout {
        header: sections[0],
        main_card: sections[1],
        footer: sections[2],
        too_small: false,
        compact,
        show_shortcut_hints,
    }
}

pub fn main_card_constraints(compact: bool) -> [Constraint; 13] {
    if compact {
        [
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(3),
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
            Constraint::Length(1),
            Constraint::Length(3),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn terminal_too_small_below_minimum() {
        assert!(terminal_too_small(rect(59, 24)));
        assert!(terminal_too_small(rect(60, 23)));
        assert!(!terminal_too_small(rect(60, 24)));
    }

    #[test]
    fn compute_layout_too_small_gate() {
        let layout = compute_layout(rect(50, 20));
        assert!(layout.too_small);
        assert_eq!(layout.header, Rect::default());
    }

    #[test]
    fn compute_layout_fits_within_parent() {
        let area = rect(100, 50);
        let layout = compute_layout(area);
        assert!(!layout.too_small);
        assert!(layout.header.y >= area.y);
        assert!(layout.header.y + layout.header.height <= area.y + area.height);
        assert!(layout.main_card.y + layout.main_card.height <= area.y + area.height);
        assert!(layout.footer.y + layout.footer.height <= area.y + area.height);
    }

    #[test]
    fn show_shortcut_hints_when_terminal_tall_enough() {
        let layout = compute_layout(rect(100, 40));
        assert!(layout.show_shortcut_hints);
        let small = compute_layout(rect(100, 28));
        assert!(!small.show_shortcut_hints);
    }

    #[test]
    fn centered_rect_never_exceeds_area() {
        let area = rect(40, 20);
        let popup = centered_rect(100, 100, area);
        assert!(popup.width <= area.width);
        assert!(popup.height <= area.height);
        assert!(popup.x >= area.x);
        assert!(popup.y >= area.y);
    }
}
