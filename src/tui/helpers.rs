use crate::tui::app::Operation;
use liblitho::devices::DeviceInfo;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::path::Path;

const TEXT: Color = Color::Rgb(228, 228, 231);
const MUTED: Color = Color::Rgb(113, 113, 122);
const ACCENT: Color = Color::Rgb(96, 165, 250);
const EMERALD: Color = Color::Rgb(52, 211, 153);
const AMBER: Color = Color::Rgb(251, 191, 36);

pub fn default_device_index(devices: &[DeviceInfo]) -> usize {
    devices.iter().position(|d| d.removable == 1).unwrap_or(0)
}

pub fn device_path(device: &DeviceInfo) -> String {
    device.device_name.clone()
}

pub fn device_display_name(device: &DeviceInfo) -> String {
    let vendor_model = format!("{} {}", device.vendor_name.trim(), device.model_name.trim())
        .trim()
        .to_string();
    if vendor_model.is_empty() {
        device.device_name.clone()
    } else {
        vendor_model
    }
}

pub fn device_label(device: &DeviceInfo) -> String {
    let removable = if device.removable == 1 {
        "(Removable)"
    } else {
        "(Fixed)"
    };
    format!(
        "{} • {} {}",
        device_display_name(device),
        format_size(device.size),
        removable
    )
}

pub fn device_list_entry(device: &DeviceInfo, selected: bool) -> ListItem<'static> {
    let path = device_path(device);
    let removable = if device.removable == 1 {
        Span::styled(" Removable", Style::default().fg(EMERALD))
    } else {
        Span::styled(" Fixed", Style::default().fg(AMBER))
    };

    let marker = if selected { "▸ " } else { "  " };
    let style = if selected {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };

    ListItem::new(Line::from(vec![
        Span::raw(marker),
        Span::styled(path, style),
        Span::styled(
            format!("  {}  ", device_display_name(device)),
            Style::default().fg(MUTED),
        ),
        Span::styled(format_size(device.size), Style::default().fg(MUTED)),
        removable,
    ]))
}

pub fn file_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

pub fn file_section_label(operation: Operation) -> &'static str {
    match operation {
        Operation::Flash => "SOURCE FILE",
        Operation::Clone => "OUTPUT FILE",
    }
}

pub fn format_size(size: u64) -> String {
    const SECTOR_SIZE: u64 = 512;
    let bytes = size * SECTOR_SIZE;

    if bytes >= 1_000_000_000_000 {
        format!("{:.2} TB", bytes as f64 / 1_000_000_000_000.0)
    } else if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.2} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}
