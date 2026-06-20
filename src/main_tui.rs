use crossterm::event::read;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fpicker::{FileExplorer, Theme};
use liblitho::devices::DeviceInfo;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::broadcast;

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

const PANEL_WIDTH: u16 = 80;
const HEADER_HEIGHT: u16 = 5;
const MAIN_CARD_HEIGHT: u16 = 52;

#[derive(Clone, Copy, PartialEq)]
enum Operation {
    Clone,
    Flash,
}

#[derive(Clone, Copy, PartialEq)]
enum InputFocus {
    Mode,
    Device,
    File,
    Start,
    Cancel,
}

#[derive(Clone, Copy, PartialEq)]
enum StatusState {
    Ready,
    InProgress,
    Complete,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    None,
    NonRemovableConfirm { target_index: usize },
}

struct App {
    image_file: String,
    devices: Vec<DeviceInfo>,
    selected_device_index: usize,
    operation: Operation,
    focus: InputFocus,
    is_running: bool,
    progress: f64,
    status_state: StatusState,
    status_detail: String,
    dialog: Dialog,
    progress_rx: broadcast::Receiver<String>,
    progress_tx: broadcast::Sender<String>,
    sim_cancel: Option<Arc<AtomicBool>>,
}

impl App {
    fn new() -> App {
        let devices = liblitho::devices::get_storage_devices().unwrap_or_default();
        let selected_device_index = default_device_index(&devices);

        let (tx, rx) = broadcast::channel(100);

        App {
            image_file: String::new(),
            devices,
            selected_device_index,
            operation: Operation::Flash,
            focus: InputFocus::Mode,
            is_running: false,
            progress: 0.0,
            status_state: StatusState::Ready,
            status_detail: String::from("Waiting for operation..."),
            dialog: Dialog::None,
            progress_rx: rx,
            progress_tx: tx,
            sim_cancel: None,
        }
    }

    fn selected_device(&self) -> Option<&DeviceInfo> {
        self.devices.get(self.selected_device_index)
    }

    fn set_status(&mut self, state: StatusState, detail: String) {
        self.status_state = state;
        self.status_detail = detail;
    }

    fn next_focus(&mut self) {
        if self.is_running {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Device,
                InputFocus::Device => InputFocus::File,
                InputFocus::File => InputFocus::Start,
                InputFocus::Start => InputFocus::Cancel,
                InputFocus::Cancel => InputFocus::Mode,
            };
        } else {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Device,
                InputFocus::Device => InputFocus::File,
                InputFocus::File => InputFocus::Start,
                InputFocus::Start => InputFocus::Mode,
                InputFocus::Cancel => InputFocus::Mode,
            };
        }
    }

    fn prev_focus(&mut self) {
        if self.is_running {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Cancel,
                InputFocus::Device => InputFocus::Mode,
                InputFocus::File => InputFocus::Device,
                InputFocus::Start => InputFocus::File,
                InputFocus::Cancel => InputFocus::Start,
            };
        } else {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Start,
                InputFocus::Device => InputFocus::Mode,
                InputFocus::File => InputFocus::Device,
                InputFocus::Start => InputFocus::File,
                InputFocus::Cancel => InputFocus::Start,
            };
        }
    }

    fn set_operation(&mut self, op: Operation) {
        self.operation = op;
    }

    fn try_select_device(&mut self, index: usize) {
        if index >= self.devices.len() {
            return;
        }

        let device = &self.devices[index];
        if device.removable == 1 {
            self.selected_device_index = index;
            self.set_status(
                StatusState::Ready,
                format!("Target: {}", device_label(device)),
            );
        } else {
            self.dialog = Dialog::NonRemovableConfirm { target_index: index };
        }
    }

    fn confirm_non_removable(&mut self, accept: bool) {
        if let Dialog::NonRemovableConfirm { target_index } = self.dialog {
            if accept {
                self.selected_device_index = target_index;
                if let Some(device) = self.devices.get(target_index) {
                    self.set_status(
                        StatusState::Ready,
                        format!(
                            "Warning: fixed device selected — {}",
                            device_label(device)
                        ),
                    );
                }
            }
        }
        self.dialog = Dialog::None;
    }

    fn refresh_devices(&mut self) {
        match liblitho::devices::get_storage_devices() {
            Ok(devs) => {
                self.devices = devs;
                self.selected_device_index =
                    default_device_index(&self.devices).min(self.devices.len().saturating_sub(1));
                self.set_status(StatusState::Ready, String::from("Devices refreshed"));
            }
            Err(e) => {
                self.set_status(
                    StatusState::Error,
                    format!("Failed to refresh devices: {}", e),
                );
            }
        }
    }

    fn start_simulation(&mut self) {
        if self.image_file.is_empty() {
            self.set_status(
                StatusState::Error,
                String::from("Select a source file before starting."),
            );
            return;
        }
        if self.devices.is_empty() {
            self.set_status(
                StatusState::Error,
                String::from("No storage devices available."),
            );
            return;
        }

        let device_name = self
            .selected_device()
            .map(|d| device_display_name(d))
            .unwrap_or_else(|| "device".to_string());

        let verb = match self.operation {
            Operation::Flash => "Writing",
            Operation::Clone => "Cloning",
        };

        self.is_running = true;
        self.progress = 0.0;
        self.set_status(
            StatusState::InProgress,
            format!("{} to {}... (simulation — disk writes disabled)", verb, device_name),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancel.clone();
        self.sim_cancel = Some(cancel);

        let tx = self.progress_tx.clone();
        tokio::spawn(async move {
            let mut progress = 0.0f64;
            while progress < 100.0 {
                if cancel_flag.load(Ordering::Relaxed) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(180)).await;
                progress = (progress + 8.0 + (progress as u64 % 5) as f64).min(100.0);
                let _ = tx.send(format!("{:.0}", progress));
            }
        });
    }

    fn cancel_operation(&mut self) {
        if let Some(cancel) = &self.sim_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.sim_cancel = None;
        self.is_running = false;
        self.progress = 0.0;
        self.set_status(
            StatusState::Cancelled,
            String::from("Operation cancelled by user."),
        );
    }

    fn check_progress(&mut self) {
        while let Ok(msg) = self.progress_rx.try_recv() {
            if let Ok(progress_value) = msg.parse::<f64>() {
                self.progress = progress_value;
                if self.progress >= 100.0 {
                    self.is_running = false;
                    self.sim_cancel = None;
                    let device_name = self
                        .selected_device()
                        .map(|d| device_display_name(d))
                        .unwrap_or_else(|| "device".to_string());
                    let verb = match self.operation {
                        Operation::Flash => "flashed",
                        Operation::Clone => "cloned",
                    };
                    self.set_status(
                        StatusState::Complete,
                        format!("Successfully {} {} (simulation)", verb, device_name),
                    );
                }
            }
        }
    }

    fn open_file_picker<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        match self.operation {
            Operation::Flash => self.open_flash_file_picker(terminal),
            Operation::Clone => self.open_clone_output_picker(terminal),
        }
    }

    fn open_flash_file_picker<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        let mut file_explorer = FileExplorer::with_theme(Theme::default())?;

        loop {
            terminal.draw(|f| {
                f.render_widget(&file_explorer.widget(), f.area());
                render_file_picker_hint(f, Operation::Flash);
            })?;
            let event = read()?;

            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Enter => {
                        let current = file_explorer.current();
                        if current.is_dir() {
                            file_explorer.handle(&event)?;
                        } else {
                            self.image_file = current.path().to_string_lossy().to_string();
                            let name = file_basename(&self.image_file);
                            self.set_status(
                                StatusState::Ready,
                                format!("Selected: {}", name),
                            );
                            break;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            file_explorer.handle(&event)?;
        }

        Ok(())
    }

    fn open_clone_output_picker<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        let mut file_explorer = FileExplorer::with_theme(Theme::default())?;

        loop {
            terminal.draw(|f| {
                f.render_widget(&file_explorer.widget(), f.area());
                render_file_picker_hint(f, Operation::Clone);
            })?;
            let event = read()?;

            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Enter | KeyCode::Char('n') => {
                        let cwd = file_explorer.cwd().clone();
                        if let Some(path) = self.prompt_output_filename(terminal, &cwd)? {
                            self.image_file = path;
                            let name = file_basename(&self.image_file);
                            self.set_status(
                                StatusState::Ready,
                                format!("Output file will be created: {}", name),
                            );
                            break;
                        }
                        continue;
                    }
                    _ => {}
                }

                // Enter names the output file in clone mode; arrow keys navigate via fpicker.
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('n')) {
                    continue;
                }
            }
            file_explorer.handle(&event)?;
        }

        Ok(())
    }

    fn prompt_output_filename<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        directory: &Path,
    ) -> io::Result<Option<String>> {
        let mut filename = default_clone_filename(self);
        let mut error: Option<String> = None;

        loop {
            terminal.draw(|f| {
                render_output_filename_dialog(f, directory, &filename, error.as_deref());
            })?;

            let event = read()?;
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if is_valid_filename(&filename) {
                            return Ok(Some(join_output_path(directory, &filename)));
                        }
                        error = Some(
                            "Use a simple filename like sdb-clone.img (no slashes)".to_string(),
                        );
                    }
                    KeyCode::Backspace => {
                        filename.pop();
                        error = None;
                    }
                    KeyCode::Char(c) => {
                        if !c.is_control() {
                            filename.push(c);
                            error = None;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn open_device_picker<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> io::Result<()> {
        if self.devices.is_empty() {
            self.refresh_devices();
            if self.devices.is_empty() {
                self.set_status(
                    StatusState::Error,
                    String::from("No storage devices found."),
                );
                return Ok(());
            }
        }

        let mut list_index = self
            .selected_device_index
            .min(self.devices.len().saturating_sub(1));

        loop {
            terminal.draw(|f| {
                ui(f, self);
                render_device_picker_dialog(f, self, list_index);
            })?;

            let event = read()?;
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => break,
                    KeyCode::Up => {
                        list_index = list_index.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if list_index + 1 < self.devices.len() {
                            list_index += 1;
                        }
                    }
                    KeyCode::Char('r') => {
                        self.refresh_devices();
                        list_index = list_index.min(self.devices.len().saturating_sub(1));
                    }
                    KeyCode::Enter => {
                        self.try_select_device(list_index);
                        break;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

fn default_device_index(devices: &[DeviceInfo]) -> usize {
    devices
        .iter()
        .position(|d| d.removable == 1)
        .unwrap_or(0)
}

fn device_path(device: &DeviceInfo) -> String {
    format!("/dev/{}", device.device_name)
}

fn device_display_name(device: &DeviceInfo) -> String {
    let vendor_model = format!(
        "{} {}",
        device.vendor_name.trim(),
        device.model_name.trim()
    )
    .trim()
    .to_string();
    if vendor_model.is_empty() {
        device.device_name.clone()
    } else {
        vendor_model
    }
}

fn device_list_entry(device: &DeviceInfo, selected: bool) -> ListItem<'static> {
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
        Span::styled(format!("  {}  ", device_display_name(device)), Style::default().fg(MUTED)),
        Span::styled(format_size(device.size), Style::default().fg(MUTED)),
        removable,
    ]))
}

fn device_label(device: &DeviceInfo) -> String {
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

fn file_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

fn file_section_label(operation: Operation) -> &'static str {
    match operation {
        Operation::Flash => "SOURCE FILE",
        Operation::Clone => "OUTPUT FILE",
    }
}

fn default_clone_filename(app: &App) -> String {
    let device = app
        .selected_device()
        .map(|d| d.device_name.as_str())
        .unwrap_or("disk");
    format!("{}-clone.img", device)
}

fn is_valid_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
}

fn join_output_path(directory: &Path, filename: &str) -> String {
    directory.join(filename).to_string_lossy().to_string()
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

pub async fn run_tui() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

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
                if app.dialog != Dialog::None {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app.confirm_non_removable(true);
                        }
                        KeyCode::Char('n')
                        | KeyCode::Char('N')
                        | KeyCode::Esc
                        | KeyCode::Enter => {
                            app.confirm_non_removable(false);
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => app.next_focus(),
                    KeyCode::BackTab => app.prev_focus(),
                    KeyCode::Left | KeyCode::Char('1') if app.focus == InputFocus::Mode => {
                        app.set_operation(Operation::Flash);
                    }
                    KeyCode::Right | KeyCode::Char('2') if app.focus == InputFocus::Mode => {
                        app.set_operation(Operation::Clone);
                    }
                    KeyCode::Char('d') if app.focus == InputFocus::Device => {
                        app.open_device_picker(terminal)?;
                    }
                    KeyCode::Char('r') if app.focus == InputFocus::Device => app.refresh_devices(),
                    KeyCode::Char('f') if app.focus == InputFocus::File => {
                        app.open_file_picker(terminal)?;
                    }
                    KeyCode::Enter => match app.focus {
                        InputFocus::Mode => {}
                        InputFocus::Device => app.open_device_picker(terminal)?,
                        InputFocus::File => app.open_file_picker(terminal)?,
                        InputFocus::Start if !app.is_running => app.start_simulation(),
                        InputFocus::Cancel if app.is_running => app.cancel_operation(),
                        _ => {}
                    },
                    KeyCode::Char('c') if app.is_running => app.cancel_operation(),
                    KeyCode::Esc if app.is_running => app.cancel_operation(),
                    _ => {}
                }
            }
        }

        app.check_progress();
    }
}

fn ui(f: &mut Frame, app: &App) {
    f.render_widget(Clear, f.area());
    f.render_widget(
        Block::default().style(Style::default().bg(BG)),
        f.area(),
    );

    let panel_height = HEADER_HEIGHT + MAIN_CARD_HEIGHT + 3;
    let top_pad = f.area().height.saturating_sub(panel_height) / 2;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(panel_height.min(f.area().height)),
            Constraint::Min(0),
        ])
        .split(f.area());

    let h_pad = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(PANEL_WIDTH),
            Constraint::Min(1),
        ])
        .split(outer[1]);

    let panel = h_pad[1];

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .spacing(1)
        .constraints([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Length(MAIN_CARD_HEIGHT),
            Constraint::Length(2),
        ])
        .split(panel);

    render_header(f, sections[0]);
    render_main_card(f, app, sections[1]);
    render_footer(f, sections[2]);

    if let Dialog::NonRemovableConfirm { target_index } = app.dialog {
        render_confirmation_dialog(f, app, target_index);
    }
}

fn render_header(f: &mut Frame, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1)])
        .split(area);

    let title_line = Line::from(vec![
        Span::styled(
            "lithographer",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("     "),
        Span::styled("● Connected", Style::default().fg(EMERALD)),
    ]);
    f.render_widget(
        Paragraph::new(title_line).alignment(Alignment::Left),
        rows[0],
    );
    f.render_widget(
        Paragraph::new("SD Card • NVMe • USB Writer").style(Style::default().fg(MUTED)),
        rows[1],
    );
}

fn render_main_card(f: &mut Frame, app: &App, area: Rect) {
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
        .constraints([
            Constraint::Length(1),  // OPERATION MODE label
            Constraint::Length(7),  // mode cards
            Constraint::Length(1),  // TARGET DEVICE label
            Constraint::Length(4),  // device select
            Constraint::Length(3),  // device info
            Constraint::Length(1),  // SOURCE FILE label
            Constraint::Length(5),  // file select
            Constraint::Length(5),  // status
            Constraint::Length(1),  // PROGRESS label
            Constraint::Length(3),  // progress bar
            Constraint::Length(4),  // controls
            Constraint::Length(2),  // shortcut hints
        ])
        .split(card_area);

    f.render_widget(section_label("OPERATION MODE"), chunks[0]);
    render_mode_cards(f, app, chunks[1]);

    f.render_widget(section_label("TARGET DEVICE"), chunks[2]);
    render_device_select(f, app, chunks[3]);
    render_device_info(f, app, chunks[4]);

    f.render_widget(section_label(file_section_label(app.operation)), chunks[5]);
    render_file_select(f, app, chunks[6]);

    render_status(f, app, chunks[7]);

    f.render_widget(section_label("PROGRESS"), chunks[8]);
    render_progress(f, app, chunks[9]);

    render_controls(f, app, chunks[10]);

    f.render_widget(
        Paragraph::new(shortcut_hints(app.is_running))
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: true }),
        chunks[11],
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
        .title(if active {
            format!(" {} ", title)
        } else {
            format!(" {} ", title)
        })
        .title_style(if active {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled(desc, Style::default().fg(MUTED))),
        Line::from(Span::styled(hint, Style::default().fg(Color::Rgb(63, 63, 70)))),
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
        (
            file_basename(&app.image_file),
            app.image_file.clone(),
        )
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
        .gauge_style(
            Style::default()
                .fg(ACCENT)
                .bg(Color::Rgb(39, 39, 42)),
        )
        .ratio(pct as f64 / 100.0)
        .label(format!("{}%", pct));

    f.render_widget(gauge, area);
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let cols = if app.is_running {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70),
                Constraint::Percentage(30),
            ])
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
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
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
        let cancel_border = if cancel_focused { RED } else { Color::Rgb(127, 29, 29) };
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

fn render_footer(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("Made with ♥ by Girish Joshi", Style::default().fg(MUTED)),
        Span::raw("   │   "),
        Span::styled("Root Required", Style::default().fg(MUTED)),
    ]);
    f.render_widget(
        Paragraph::new(line).alignment(Alignment::Center),
        area,
    );
}

fn render_device_picker_dialog(f: &mut Frame, app: &App, list_index: usize) {
    let visible_count = app.devices.len().clamp(1, 12);
    let list_height = visible_count as u16;
    let dialog_height = list_height + 5;
    let dialog_width = PANEL_WIDTH.saturating_sub(4).max(50);
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
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

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

fn render_file_picker_hint(f: &mut Frame, operation: Operation) {
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

fn render_output_filename_dialog(
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
            Span::styled(
                directory.display().to_string(),
                Style::default().fg(TEXT),
            ),
        ]),
        Line::from(Span::styled(
            "File will be created when cloning starts.",
            Style::default().fg(MUTED),
        )),
        Line::from(vec![
            Span::styled("Filename: ", Style::default().fg(MUTED)),
            Span::styled(filename, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
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

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn shortcut_hints(running: bool) -> String {
    if running {
        "Tab: focus · Enter: action · c/Esc: cancel · q: quit".to_string()
    } else {
        "Tab: focus · ←/→ or 1/2: mode · Enter/d: pick device · r: refresh · Enter/f: pick file · Enter: start · q: quit".to_string()
    }
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    run_tui().await
}