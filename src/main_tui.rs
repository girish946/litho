use crossterm::event::read;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fpicker::{FileExplorer, Theme};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;
use tokio::sync::broadcast;

const BLOCKS: [u64; 14] = [
    4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608,
    16777216, 33554432,
];

#[derive(Clone, Copy, PartialEq)]
enum Operation {
    Clone,
    Flash,
}

#[derive(Clone, Copy, PartialEq)]
enum InputFocus {
    ImageFile,
    DeviceFile,
    Operation,
    StartStop,
}

struct App {
    image_file: String,
    device_file: String,
    available_devices: Vec<String>,
    selected_device_index: usize,
    operation: Operation,
    focus: InputFocus,
    is_running: bool,
    progress: f64,
    status_message: String,
    status_messages: Vec<String>,
    progress_rx: broadcast::Receiver<String>,
    progress_tx: broadcast::Sender<String>,
    error_rx: broadcast::Receiver<String>,
    error_tx: broadcast::Sender<String>,
}

impl App {
    fn new() -> App {
        let devices = match liblitho::devices::get_storage_devices() {
            Ok(devs) => devs
                .into_iter()
                .filter(|d| d.removable == 1)
                .map(|d| {
                    format!(
                        "{} - {} {} ({})",
                        d.device_name,
                        d.vendor_name.trim(),
                        d.model_name.trim(),
                        format_size(d.size)
                    )
                })
                .collect(),
            Err(_) => vec![],
        };

        let (tx, rx) = broadcast::channel(100);
        let (error_tx, error_rx) = broadcast::channel(10);

        App {
            image_file: String::new(),
            device_file: String::new(),
            available_devices: devices,
            selected_device_index: 0,
            operation: Operation::Clone,
            focus: InputFocus::ImageFile,
            is_running: false,
            progress: 0.0,
            status_message: String::from("Ready"),
            status_messages: vec![String::from("Ready")],
            progress_rx: rx,
            progress_tx: tx,
            error_rx,
            error_tx,
        }
    }

    fn add_status_message(&mut self, message: String) {
        self.status_message = message.clone();
        self.status_messages.push(message);
        // Keep only last 100 messages to avoid memory issues
        if self.status_messages.len() > 100 {
            self.status_messages.remove(0);
        }
    }

    fn next_focus(&mut self) {
        self.focus = match self.focus {
            InputFocus::ImageFile => InputFocus::DeviceFile,
            InputFocus::DeviceFile => InputFocus::Operation,
            InputFocus::Operation => InputFocus::StartStop,
            InputFocus::StartStop => InputFocus::ImageFile,
        };
    }

    fn prev_focus(&mut self) {
        self.focus = match self.focus {
            InputFocus::ImageFile => InputFocus::StartStop,
            InputFocus::DeviceFile => InputFocus::ImageFile,
            InputFocus::Operation => InputFocus::DeviceFile,
            InputFocus::StartStop => InputFocus::Operation,
        };
    }

    fn toggle_operation(&mut self) {
        self.operation = match self.operation {
            Operation::Clone => Operation::Flash,
            Operation::Flash => Operation::Clone,
        };
    }

    fn next_device(&mut self) {
        if !self.available_devices.is_empty() {
            self.selected_device_index =
                (self.selected_device_index + 1) % self.available_devices.len();
            self.device_file = self.available_devices[self.selected_device_index].clone();
            let device_name = self.device_file.split(" - ").next().unwrap_or("");
            self.add_status_message(format!("Device selected: {}", device_name));
        }
    }

    fn prev_device(&mut self) {
        if !self.available_devices.is_empty() {
            if self.selected_device_index == 0 {
                self.selected_device_index = self.available_devices.len() - 1;
            } else {
                self.selected_device_index -= 1;
            }
            self.device_file = self.available_devices[self.selected_device_index].clone();
            let device_name = self.device_file.split(" - ").next().unwrap_or("");
            self.add_status_message(format!("Device selected: {}", device_name));
        }
    }

    fn refresh_devices(&mut self) {
        match liblitho::devices::get_storage_devices() {
            Ok(devs) => {
                self.available_devices = devs
                    .into_iter()
                    .filter(|d| d.removable == 1)
                    .map(|d| {
                        format!(
                            "{} - {} {} ({})",
                            d.device_name,
                            d.vendor_name.trim(),
                            d.model_name.trim(),
                            format_size(d.size)
                        )
                    })
                    .collect();
                self.selected_device_index = 0;
                if !self.available_devices.is_empty() {
                    self.device_file = self.available_devices[0]
                        .split(" - ")
                        .next()
                        .unwrap_or("")
                        .to_string();
                }
                self.add_status_message(String::from("Devices refreshed"));
            }
            Err(e) => {
                self.add_status_message(format!("Failed to refresh devices: {}", e));
            }
        }
    }

    fn toggle_running(&mut self) {
        if !self.is_running && !self.image_file.is_empty() && !self.device_file.is_empty() {
            self.is_running = true;
            self.progress = 0.0;
            let operation_name = match self.operation {
                Operation::Clone => "Cloning",
                Operation::Flash => "Flashing",
            };
            let device_name = self.device_file.split(" - ").next().unwrap_or(&self.device_file);
            self.add_status_message(format!(
                "{} {} to {} started...",
                operation_name,
                self.image_file,
                device_name
            ));
            
            // Spawn the actual operation
            self.start_operation();
        } else if self.is_running {
            self.is_running = false;
            self.add_status_message(String::from("Operation stopped"));
        }
    }

    fn start_operation(&mut self) {
        let image_file = self.image_file.clone();
        let device_file = self.device_file.split(" - ").next().unwrap_or(&self.device_file).to_string();
        let operation = self.operation;
        let tx = self.progress_tx.clone();
        let error_tx = self.error_tx.clone();

        // Calculate optimal block size and add to status
        let block_size = calculate_block_size(&image_file, &device_file, operation);
        self.add_status_message(format!("Using block size: {} bytes ({})", block_size, format_size_bytes(block_size as u64)));

        tokio::spawn(async move {
            // Calculate optimal block size
            let block_size = calculate_block_size(&image_file, &device_file, operation);
            
            
            let result = match operation {
                Operation::Clone => {
                    liblitho::clone(
                        device_file,
                        image_file,
                        block_size,
                        false,
                        None::<fn(f64)>,
                        Some(tx),
                    )
                }
                Operation::Flash => {
                    liblitho::flash(
                        image_file,
                        device_file,
                        block_size,
                        false,
                        None::<fn(f64)>,
                        Some(tx),
                    )
                }
            };

            if let Err(e) = result {
                log::error!("Operation failed: {}", e);
                let _ = error_tx.send(format!("Operation failed: {}", e));
            }
        });
    }

    fn check_progress(&mut self) {
        // Try to receive progress updates without blocking
        while let Ok(msg) = self.progress_rx.try_recv() {
            if let Ok(progress_value) = msg.parse::<f64>() {
                self.progress = progress_value;
                if self.progress >= 100.0 {
                    self.is_running = false;
                    self.add_status_message(String::from("Operation completed!"));
                }
            }
        }

        // Check for error messages
        while let Ok(error_msg) = self.error_rx.try_recv() {
            self.is_running = false;
            self.add_status_message(error_msg);
            self.progress = 0.0;
        }
    }

    fn open_file_picker<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        let theme = Theme::default();

        let mut file_explorer = FileExplorer::with_theme(theme)?;

        loop {
            terminal.draw(|f| {
                let area = f.area();
                f.render_widget(&file_explorer.widget(), area);
            })?;
            let event = read()?;

            if let Event::Key(key) = event {
                if key.code == KeyCode::Char('q') {
                    break;
                } else if key.code == KeyCode::Esc {
                    break;
                } else if key.code == KeyCode::Enter {
                    if let Some(selected) = file_explorer.selected_files().first() {
                        if !selected.is_dir() {
                            self.image_file = selected.path().to_string_lossy().to_string();
                            self.add_status_message(format!("File selected: {}", self.image_file));
                        }
                        break;
                    }
                }
            }
            file_explorer.handle(&event)?;
        }

        Ok(())
    }
}

fn format_size_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.2} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_size(size: u64) -> String {
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

pub async fn run_tui() -> Result<(), io::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('r') if app.focus == InputFocus::DeviceFile => {
                        app.refresh_devices();
                    }
                    KeyCode::Char('f') if app.focus == InputFocus::ImageFile => {
                        app.open_file_picker(terminal)?;
                    }
                    KeyCode::Tab => app.next_focus(),
                    KeyCode::BackTab => app.prev_focus(),
                    KeyCode::Up => {
                        if app.focus == InputFocus::DeviceFile {
                            app.prev_device();
                        }
                    }
                    KeyCode::Down => {
                        if app.focus == InputFocus::DeviceFile {
                            app.next_device();
                        }
                    }
                    KeyCode::Enter => {
                        if app.focus == InputFocus::StartStop {
                            app.toggle_running();
                        } else if app.focus == InputFocus::Operation {
                            app.toggle_operation();
                        } else if app.focus == InputFocus::ImageFile {
                            app.open_file_picker(terminal)?;
                        }
                    }
                    KeyCode::Char(c) => {
                        if app.focus == InputFocus::ImageFile && c != 'f' {
                            app.image_file.push(c);
                        } else if app.focus == InputFocus::Operation && (c == ' ') {
                            app.toggle_operation();
                        }
                    }
                    KeyCode::Backspace => {
                        if app.focus == InputFocus::ImageFile {
                            app.image_file.pop();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for progress updates from the channel
        app.check_progress();
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(f.area());

    // Image file input
    let image_style = if app.focus == InputFocus::ImageFile {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let image_input = Paragraph::new(app.image_file.as_str())
        .style(image_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Image File (Enter or 'f' to browse)"),
        );
    f.render_widget(image_input, chunks[0]);

    // Device file dropdown
    let device_style = if app.focus == InputFocus::DeviceFile {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let device_items: Vec<ListItem> = app
        .available_devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let marker = if i == app.selected_device_index {
                "→ "
            } else {
                "  "
            };
            let style = if i == app.selected_device_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{}{}", marker, device)).style(style)
        })
        .collect();

    let device_list = List::new(device_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Device File (↑/↓ to select, 'r' to refresh)"),
        )
        .style(device_style);
    f.render_widget(device_list, chunks[1]);

    // Operation selection
    let op_style = if app.focus == InputFocus::Operation {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let clone_marker = if app.operation == Operation::Clone {
        "(*)"
    } else {
        "( )"
    };
    let flash_marker = if app.operation == Operation::Flash {
        "(*)"
    } else {
        "( )"
    };
    let operations = vec![
        ListItem::new(format!("{} Clone", clone_marker)),
        ListItem::new(format!("{} Flash", flash_marker)),
    ];
    let operation_list = List::new(operations)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Operation (Space to toggle)"),
        )
        .style(op_style);
    f.render_widget(operation_list, chunks[2]);

    // Start/Stop button
    let button_style = if app.focus == InputFocus::StartStop {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let button_text = if app.is_running { "STOP" } else { "START" };
    let button = Paragraph::new(button_text).style(button_style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Action (Enter to toggle)"),
    );
    f.render_widget(button, chunks[3]);

    // Progress bar
    let progress_bar = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .percent(app.progress as u16);
    f.render_widget(progress_bar, chunks[4]);

    // Status message - show multiple recent messages
    let status_text = app.status_messages.join("\n");
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[5]);
}

fn calculate_block_size(image_file: &str, device_file: &str, operation: Operation) -> usize {
    let size = match operation {
        Operation::Flash => {
            // For flash, use image file size
            std::fs::metadata(image_file)
                .ok()
                .map(|m| m.len())
                .unwrap_or(0)
        }
        Operation::Clone => {
            // For clone, try to get device size
            if let Ok(devices) = liblitho::devices::get_storage_devices() {
                let device_name = device_file.split('/').last().unwrap_or("");
                devices
                    .into_iter()
                    .find(|d| d.device_name == device_name)
                    .map(|d| d.size * 512) // Convert sectors to bytes
                    .unwrap_or(0)
            } else {
                0
            }
        }
    };

    // Find optimal block size (approximately 1/1000th of total size, but within BLOCKS array)
    let target_block = size / 1000;
    
    // Find the closest block size from BLOCKS array
    let block_size = BLOCKS
        .iter()
        .find(|&&b| b >= target_block)
        .copied()
        .unwrap_or(*BLOCKS.last().unwrap());

    block_size as usize
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    run_tui().await
}
