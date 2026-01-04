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
    /// Creates a new App instance with initial state.
    /// 
    /// Initializes available devices by querying removable storage devices,
    /// sets up broadcast channels for progress and error reporting, and
    /// sets default values for all fields.
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

    /// Adds a status message to the message history and updates the current status.
    /// 
    /// # Arguments
    /// 
    /// * `message` - The status message to add
    /// 
    /// Maintains a rolling window of the last 100 messages to prevent unbounded memory growth.
    fn add_status_message(&mut self, message: String) {
        self.status_message = message.clone();
        self.status_messages.push(message);
        // Keep only last 100 messages to avoid memory issues
        if self.status_messages.len() > 100 {
            self.status_messages.remove(0);
        }
    }

    /// Moves focus to the next input field in the UI.
    /// 
    /// Cycles through: ImageFile -> DeviceFile -> Operation -> StartStop -> ImageFile
    fn next_focus(&mut self) {
        self.focus = match self.focus {
            InputFocus::ImageFile => InputFocus::DeviceFile,
            InputFocus::DeviceFile => InputFocus::Operation,
            InputFocus::Operation => InputFocus::StartStop,
            InputFocus::StartStop => InputFocus::ImageFile,
        };
    }

    /// Moves focus to the previous input field in the UI.
    /// 
    /// Cycles through: StartStop -> Operation -> DeviceFile -> ImageFile -> StartStop
    fn prev_focus(&mut self) {
        self.focus = match self.focus {
            InputFocus::ImageFile => InputFocus::StartStop,
            InputFocus::DeviceFile => InputFocus::ImageFile,
            InputFocus::Operation => InputFocus::DeviceFile,
            InputFocus::StartStop => InputFocus::Operation,
        };
    }

    /// Toggles between Clone and Flash operations.
    fn toggle_operation(&mut self) {
        self.operation = match self.operation {
            Operation::Clone => Operation::Flash,
            Operation::Flash => Operation::Clone,
        };
    }

    /// Selects the next available device in the device list.
    /// 
    /// Wraps around to the first device when reaching the end of the list.
    /// Updates the status message with the selected device name.
    fn next_device(&mut self) {
        if !self.available_devices.is_empty() {
            self.selected_device_index =
                (self.selected_device_index + 1) % self.available_devices.len();
            self.device_file = self.available_devices[self.selected_device_index].clone();
            let device_name = self.device_file.split(" - ").next().unwrap_or("");
            self.add_status_message(format!("Device selected: {}", device_name));
        }
    }

    /// Selects the previous available device in the device list.
    /// 
    /// Wraps around to the last device when at the beginning of the list.
    /// Updates the status message with the selected device name.
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

    /// Refreshes the list of available removable storage devices.
    /// 
    /// Queries the system for current removable devices and updates the device list.
    /// Resets the selected device index to 0 and updates the status message.
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

    /// Toggles the running state of the clone/flash operation.
    /// 
    /// If not running and both image file and device file are selected:
    /// - Starts the operation by spawning an async task
    /// - Updates progress to 0%
    /// - Sets status message with operation details
    /// 
    /// If already running:
    /// - Stops the operation
    /// - Updates status message
    fn toggle_running(&mut self) {
        if !self.is_running && !self.image_file.is_empty() && !self.device_file.is_empty() {
            self.is_running = true;
            self.progress = 0.0;
            let operation_name = match self.operation {
                Operation::Clone => "Cloning",
                Operation::Flash => "Flashing",
            };
            let device_name = self
                .device_file
                .split(" - ")
                .next()
                .unwrap_or(&self.device_file);
            self.add_status_message(format!(
                "{} {} to {} started...",
                operation_name, self.image_file, device_name
            ));

            // Spawn the actual operation
            self.start_operation();
        } else if self.is_running {
            self.is_running = false;
            self.add_status_message(String::from("Operation stopped"));
        }
    }

    /// Spawns an async task to perform the clone or flash operation.
    /// 
    /// Calculates optimal block size based on file/device size,
    /// updates status message with block size information,
    /// and executes the appropriate operation (clone or flash).
    /// 
    /// Errors are sent through the error channel to be displayed in the UI.
    fn start_operation(&mut self) {
        let image_file = self.image_file.clone();
        let device_file = self
            .device_file
            .split(" - ")
            .next()
            .unwrap_or(&self.device_file)
            .to_string();
        let operation = self.operation;
        let tx = self.progress_tx.clone();
        let error_tx = self.error_tx.clone();

        // Calculate optimal block size and add to status
        let block_size = calculate_block_size(&image_file, &device_file, operation);
        self.add_status_message(format!(
            "Using block size: {} bytes ({})",
            block_size,
            format_size_bytes(block_size as u64)
        ));

        tokio::spawn(async move {
            // Calculate optimal block size
            let block_size = calculate_block_size(&image_file, &device_file, operation);

            let result = match operation {
                Operation::Clone => liblitho::clone(
                    device_file,
                    image_file,
                    block_size,
                    false,
                    None::<fn(f64)>,
                    Some(tx),
                ),
                Operation::Flash => liblitho::flash(
                    image_file,
                    device_file,
                    block_size,
                    false,
                    None::<fn(f64)>,
                    Some(tx),
                ),
            };

            if let Err(e) = result {
                log::error!("Operation failed: {}", e);
                let _ = error_tx.send(format!("Operation failed: {}", e));
            }
        });
    }

    /// Checks for progress updates and error messages from the operation channels.
    /// 
    /// Non-blocking check that:
    /// - Updates progress percentage from progress channel
    /// - Marks operation as complete when reaching 100%
    /// - Handles error messages from error channel
    /// - Resets progress on error
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

    /// Opens an interactive file picker for selecting an image file.
    /// 
    /// # Arguments
    /// 
    /// * `terminal` - The terminal backend to render the file picker
    /// 
    /// # Returns
    /// 
    /// * `io::Result<()>` - Ok if successful, Err on IO errors
    /// 
    /// The file picker supports:
    /// - Navigation with arrow keys and Enter
    /// - Quitting with 'q' or Escape
    /// - Only files (not directories) can be selected
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

/// Formats a byte count into a human-readable string with appropriate units.
/// 
/// # Arguments
/// 
/// * `bytes` - Number of bytes to format
/// 
/// # Returns
/// 
/// * `String` - Formatted string with units (B, KB, MB, or GB)
/// 
/// Uses decimal (base-1000) units.
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

/// Formats a sector count into a human-readable string with appropriate units.
/// 
/// # Arguments
/// 
/// * `size` - Number of 512-byte sectors
/// 
/// # Returns
/// 
/// * `String` - Formatted string with units (B, KB, MB, GB, or TB)
/// 
/// Converts sectors to bytes (assuming 512-byte sectors) and uses decimal units.
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

/// Initializes and runs the terminal user interface.
/// 
/// # Returns
/// 
/// * `Result<(), io::Error>` - Ok if successful, Err on terminal or IO errors
/// 
/// Sets up the terminal in raw mode with alternate screen and mouse capture,
/// runs the main application loop, and ensures proper cleanup on exit.
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

/// Main application loop that handles events and renders the UI.
/// 
/// # Arguments
/// 
/// * `terminal` - The terminal backend for rendering
/// * `app` - Mutable reference to the application state
/// 
/// # Returns
/// 
/// * `io::Result<()>` - Ok if successful, Err on IO errors
/// 
/// Polls for keyboard events every 100ms, handles user input,
/// updates application state, and re-renders the UI.
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

/// Renders the user interface with all widgets.
/// 
/// # Arguments
/// 
/// * `f` - Frame to render widgets into
/// * `app` - Application state to display
/// 
/// Creates a vertical layout with:
/// - Title bar
/// - Image file input field
/// - Device selection list
/// - Operation selection (Clone/Flash)
/// - Start/Stop button
/// - Progress bar
/// - Status message area
fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("litho-tui")
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(title, chunks[0]);

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
    f.render_widget(image_input, chunks[1]);

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
    f.render_widget(device_list, chunks[2]);

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
    f.render_widget(operation_list, chunks[3]);

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
    f.render_widget(button, chunks[4]);

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
    f.render_widget(progress_bar, chunks[5]);

    // Status message - show multiple recent messages
    let status_text = app.status_messages.join("\n");
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[6]);
}

/// Calculates the optimal block size for read/write operations.
/// 
/// # Arguments
/// 
/// * `image_file` - Path to the image file
/// * `device_file` - Path to the device file
/// * `operation` - The operation type (Clone or Flash)
/// 
/// # Returns
/// 
/// * `usize` - Optimal block size in bytes, aligned to predefined block sizes
/// 
/// Strategy:
/// - For Flash: Uses image file size
/// - For Clone: Uses device size
/// - Targets approximately 1/1000th of total size
/// - Selects from predefined BLOCKS array (4KB to 32MB)
/// - Falls back to largest block size if target exceeds all options
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

/// Entry point for the TUI application.
/// 
/// # Returns
/// 
/// * `Result<(), io::Error>` - Ok if successful, Err on errors
/// 
/// Initializes the tokio runtime and starts the TUI.
#[tokio::main]
async fn main() -> Result<(), io::Error> {
    run_tui().await
}
