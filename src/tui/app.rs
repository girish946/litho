use crate::tui::helpers::{
    default_device_index, device_display_name, device_label, device_path, file_basename,
};
use crate::tui::launch::{launch_prefilled, LaunchParams};
use crate::tui::layout::{terminal_too_small, MIN_COLS, MIN_ROWS};
use crate::tui::operation::spawn_operation;
use liblitho::io_backend::{complete_suffix, in_progress_suffix};
use crate::tui::privilege::{is_running_as_root, polkit_agent_available, relaunch_elevated};
use crate::tui::ui::{
    render_device_picker_dialog, render_file_picker_hint, render_output_filename_dialog, ui,
};
use crossterm::event::read;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fpicker::{FileExplorer, Theme};
use liblitho::devices::DeviceInfo;
use liblitho::progress::{OperationPhase, OperationProgress};
use log::{error, info};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::io::{self, IsTerminal, Stdout};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Clone,
    Flash,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputFocus {
    Mode,
    Device,
    File,
    Verify,
    Start,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatusState {
    Ready,
    InProgress,
    Complete,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    None,
    NonRemovableConfirm { target_index: usize },
    ElevationConfirm,
}

pub struct App {
    pub image_file: String,
    pub devices: Vec<DeviceInfo>,
    pub selected_device_index: usize,
    pub operation: Operation,
    pub focus: InputFocus,
    pub is_running: bool,
    pub progress: f64,
    pub status_state: StatusState,
    pub status_detail: String,
    pub dialog: Dialog,
    pub progress_rx: Receiver<OperationProgress>,
    pub operation_cancel: Arc<AtomicBool>,
    pub is_root: bool,
    pub polkit_available: bool,
    pub auto_start_pending: bool,
    pub terminal_warn_logged: bool,
    /// When true, flash operations read the device back and compare checksums.
    pub verify_checksum: bool,
}

impl App {
    pub fn new(launch: LaunchParams) -> App {
        let devices = load_storage_devices(launch.device.is_some());
        let mut selected_device_index = default_device_index(&devices);

        let operation = match launch.mode.as_deref() {
            Some(mode) if mode == "clone" => Operation::Clone,
            _ => Operation::Flash,
        };

        if let Some(ref wanted_device) = launch.device {
            if let Some(idx) = devices
                .iter()
                .position(|d| device_path(d) == *wanted_device)
            {
                selected_device_index = idx;
            }
        }

        let image_prefilled = launch.image.as_ref().is_some_and(|path| !path.is_empty());
        let is_root = is_running_as_root();
        let auto_start_pending = launch.start && is_root;
        let polkit_available = is_root || polkit_agent_available();
        let focus = if launch_prefilled(&launch, image_prefilled) && !auto_start_pending {
            InputFocus::Start
        } else {
            InputFocus::Mode
        };

        let image_file = launch.image.unwrap_or_default();
        let (_tx, progress_rx) = mpsc::channel();

        App {
            image_file,
            devices,
            selected_device_index,
            operation,
            focus,
            is_running: false,
            progress: 0.0,
            status_state: StatusState::Ready,
            status_detail: String::from("Waiting for operation..."),
            dialog: Dialog::None,
            progress_rx,
            operation_cancel: Arc::new(AtomicBool::new(false)),
            is_root,
            polkit_available,
            auto_start_pending,
            terminal_warn_logged: false,
            verify_checksum: false,
        }
    }

    pub fn selected_device(&self) -> Option<&DeviceInfo> {
        self.devices.get(self.selected_device_index)
    }

    pub fn set_status(&mut self, state: StatusState, detail: String) {
        self.status_state = state;
        self.status_detail = detail;
    }

    pub fn next_focus(&mut self) {
        if self.is_running {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Device,
                InputFocus::Device => InputFocus::File,
                InputFocus::File => self.next_after_file(),
                InputFocus::Verify => InputFocus::Start,
                InputFocus::Start => InputFocus::Cancel,
                InputFocus::Cancel => InputFocus::Mode,
            };
        } else {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Device,
                InputFocus::Device => InputFocus::File,
                InputFocus::File => self.next_after_file(),
                InputFocus::Verify => InputFocus::Start,
                InputFocus::Start => InputFocus::Mode,
                InputFocus::Cancel => InputFocus::Mode,
            };
        }
    }

    pub fn prev_focus(&mut self) {
        if self.is_running {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Cancel,
                InputFocus::Device => InputFocus::Mode,
                InputFocus::File => InputFocus::Device,
                InputFocus::Verify => InputFocus::File,
                InputFocus::Start => self.prev_before_start(),
                InputFocus::Cancel => InputFocus::Start,
            };
        } else {
            self.focus = match self.focus {
                InputFocus::Mode => InputFocus::Start,
                InputFocus::Device => InputFocus::Mode,
                InputFocus::File => InputFocus::Device,
                InputFocus::Verify => InputFocus::File,
                InputFocus::Start => self.prev_before_start(),
                InputFocus::Cancel => InputFocus::Start,
            };
        }
    }

    fn next_after_file(&self) -> InputFocus {
        if self.operation == Operation::Flash {
            InputFocus::Verify
        } else {
            InputFocus::Start
        }
    }

    fn prev_before_start(&self) -> InputFocus {
        if self.operation == Operation::Flash {
            InputFocus::Verify
        } else {
            InputFocus::File
        }
    }

    pub fn set_operation(&mut self, op: Operation) {
        self.operation = op;
        if op == Operation::Clone && self.focus == InputFocus::Verify {
            self.focus = InputFocus::File;
        }
    }

    pub fn toggle_verify_checksum(&mut self) {
        if self.operation == Operation::Flash && !self.is_running {
            self.verify_checksum = !self.verify_checksum;
        }
    }

    pub fn try_select_device(&mut self, index: usize) {
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
            self.dialog = Dialog::NonRemovableConfirm {
                target_index: index,
            };
        }
    }

    pub fn confirm_non_removable(&mut self, accept: bool) {
        if let Dialog::NonRemovableConfirm { target_index } = self.dialog {
            if accept {
                self.selected_device_index = target_index;
                if let Some(device) = self.devices.get(target_index) {
                    self.set_status(
                        StatusState::Ready,
                        format!("Warning: fixed device selected — {}", device_label(device)),
                    );
                }
            }
        }
        self.dialog = Dialog::None;
    }

    pub fn confirm_elevation(&mut self, accept: bool, terminal: &mut TuiTerminal) {
        if accept {
            if !crate::tui::privilege::pkexec_on_path() {
                self.set_status(
                    StatusState::Error,
                    String::from("pkexec not found. Install polkit (pkexec)."),
                );
                self.dialog = Dialog::None;
                return;
            }
            if crate::tui::privilege::find_polkit_auth_agent().is_none() {
                self.set_status(
                    StatusState::Error,
                    String::from(
                        "No polkit authentication agent found. Start your desktop polkit agent.",
                    ),
                );
                self.dialog = Dialog::None;
                return;
            }

            let mode = match self.operation {
                Operation::Flash => "flash",
                Operation::Clone => "clone",
            };
            let device = self.selected_device().map(device_path).unwrap_or_default();
            let image = self.image_file.clone();

            if device.is_empty() || image.is_empty() {
                self.set_status(
                    StatusState::Error,
                    String::from("Select a device and file before elevating privileges."),
                );
                self.dialog = Dialog::None;
                return;
            }

            let known: Vec<&str> = self.devices.iter().map(|d| d.device_name.as_str()).collect();
            if let Err(e) = liblitho::devices::validate_listed_block_device(&device, &known) {
                self.set_status(StatusState::Error, e);
                self.dialog = Dialog::None;
                return;
            }

            info!(
                "Requesting elevation via pkexec: mode={}, device={}, image={}",
                mode, device, image
            );
            teardown_terminal(terminal);

            match relaunch_elevated(mode, &device, &image) {
                Ok(()) => {
                    // Non-Unix: elevated child was spawned; this process must exit.
                    #[cfg(not(unix))]
                    std::process::exit(0);
                }
                Err(e) => {
                    error!("Failed to relaunch elevated: {e}");
                    if let Err(re) = resume_terminal(terminal) {
                        error!("Terminal recovery after failed elevation: {re:?}");
                        eprintln!("{e}\nTerminal could not be restored. Run litho-tui again.");
                        std::process::exit(1);
                    }
                    self.set_status(StatusState::Error, format!("Elevation failed: {e}"));
                }
            }
        }
        self.dialog = Dialog::None;
    }

    pub fn refresh_devices(&mut self) {
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

    pub fn start_operation(&mut self) {
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

        if !self.is_root {
            if self.dialog == Dialog::None {
                self.dialog = Dialog::ElevationConfirm;
            }
            return;
        }

        let device_path = self.selected_device().map(device_path).unwrap_or_default();
        if device_path.is_empty() {
            self.set_status(
                StatusState::Error,
                String::from("Select a storage device before starting."),
            );
            return;
        }

        let known: Vec<&str> = self.devices.iter().map(|d| d.device_name.as_str()).collect();
        if let Err(e) = liblitho::devices::validate_listed_block_device(&device_path, &known) {
            self.set_status(StatusState::Error, e);
            return;
        }

        let device_name = self
            .selected_device()
            .map(device_display_name)
            .unwrap_or_else(|| "device".to_string());

        let verb = match self.operation {
            Operation::Flash => "Writing",
            Operation::Clone => "Cloning",
        };

        self.is_running = true;
        self.progress = 0.0;
        self.auto_start_pending = false;
        self.set_status(
            StatusState::InProgress,
            format!("{verb} to {device_name}...{suffix}", suffix = in_progress_suffix()),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        self.operation_cancel = cancel.clone();

        let (tx, rx) = mpsc::channel();
        self.progress_rx = rx;

        let image_path = self.image_file.clone();
        let op = self.operation;

        let block_size = self
            .selected_device()
            .map(|d| liblitho::devices::optimal_io_block_size_from_sectors(d.size))
            .unwrap_or_else(|| liblitho::devices::optimal_io_block_size(&device_path));

        info!(
            "Starting {} to {} (image={}, block_size={})",
            match op {
                Operation::Flash => "flash",
                Operation::Clone => "clone",
            },
            device_path,
            image_path,
            block_size
        );

        let verify = self.operation == Operation::Flash && self.verify_checksum;
        spawn_operation(op, device_path, image_path, block_size, verify, cancel, tx);
    }

    pub fn cancel_operation(&mut self) {
        self.operation_cancel.store(true, Ordering::Relaxed);
        info!("Operation cancel requested");
        self.set_status(
            StatusState::InProgress,
            String::from("Cancelling — waiting for I/O to stop..."),
        );
    }

    pub fn check_progress(&mut self) {
        loop {
            match self.progress_rx.try_recv() {
                Ok(progress) => {
                    if self.operation_cancel.load(Ordering::Relaxed)
                        && progress.phase != OperationPhase::Cancelled
                    {
                        continue;
                    }
                    self.apply_progress(progress);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.is_running {
                        self.is_running = false;
                        if self.operation_cancel.load(Ordering::Relaxed) {
                            self.set_status(
                                StatusState::Cancelled,
                                String::from("Operation cancelled by user."),
                            );
                        } else {
                            self.set_status(
                                StatusState::Error,
                                String::from("Operation ended unexpectedly."),
                            );
                        }
                    }
                    break;
                }
            }
        }
    }

    fn apply_progress(&mut self, progress: OperationProgress) {
        if self.operation_cancel.load(Ordering::Relaxed)
            && progress.phase != OperationPhase::Cancelled
        {
            return;
        }

        if let Some(pct) = progress.percentage {
            self.progress = pct.clamp(0.0, 100.0);
        }

        if let Some(ref message) = progress.message {
            self.status_detail = message.clone();
        } else {
            self.status_detail = phase_detail(&progress);
        }

        match progress.phase {
            OperationPhase::Preparing
            | OperationPhase::Decompressing
            | OperationPhase::Writing
            | OperationPhase::Verifying => {
                self.status_state = StatusState::InProgress;
            }
            OperationPhase::Complete => {
                self.is_running = false;
                self.progress = 100.0;
                let device_name = self
                    .selected_device()
                    .map(device_display_name)
                    .unwrap_or_else(|| "device".to_string());
                let verb = match self.operation {
                    Operation::Flash => "flashed",
                    Operation::Clone => "cloned",
                };
                self.set_status(
                    StatusState::Complete,
                    format!("Successfully {verb} {device_name}{suffix}", suffix = complete_suffix()),
                );
            }
            OperationPhase::Failed => {
                self.is_running = false;
                self.set_status(
                    StatusState::Error,
                    progress
                        .message
                        .unwrap_or_else(|| String::from("Operation failed.")),
                );
            }
            OperationPhase::Cancelled => {
                self.is_running = false;
                self.set_status(
                    StatusState::Cancelled,
                    progress
                        .message
                        .unwrap_or_else(|| String::from("Operation cancelled by user.")),
                );
            }
        }
    }

    pub fn open_file_picker<B: ratatui::backend::Backend>(
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
                            self.set_status(StatusState::Ready, format!("Selected: {}", name));
                            break;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            file_explorer.handle(&event)?;
        }

        terminal.draw(|f| ui(f, self))?;
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

                if matches!(key.code, KeyCode::Enter | KeyCode::Char('n')) {
                    continue;
                }
            }
            file_explorer.handle(&event)?;
        }

        terminal.draw(|f| ui(f, self))?;
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

    pub fn open_device_picker<B: ratatui::backend::Backend>(
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

fn load_storage_devices(refresh_for_prefill: bool) -> Vec<DeviceInfo> {
    if refresh_for_prefill {
        info!("Refreshing device list for --device pre-fill");
    }
    liblitho::devices::get_storage_devices().unwrap_or_else(|e| {
        if refresh_for_prefill {
            log::warn!("Failed to load devices for pre-fill: {e}");
        }
        Vec::new()
    })
}

fn default_clone_filename(app: &App) -> String {
    let device = app
        .selected_device()
        .map(|d| {
            Path::new(&d.device_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("disk")
        })
        .unwrap_or("disk");
    format!("{}-clone.img", device)
}

fn is_valid_filename(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

fn join_output_path(directory: &Path, filename: &str) -> String {
    directory.join(filename).to_string_lossy().to_string()
}

fn phase_detail(progress: &OperationProgress) -> String {
    match progress.phase {
        OperationPhase::Preparing => "Preparing...".to_string(),
        OperationPhase::Decompressing => "Decompressing image...".to_string(),
        OperationPhase::Writing => {
            if let Some(total) = progress.bytes_total {
                format!("Writing... {} / {} bytes", progress.bytes_processed, total)
            } else {
                format!("Writing... {} bytes", progress.bytes_processed)
            }
        }
        OperationPhase::Verifying => "Verifying checksum...".to_string(),
        OperationPhase::Complete => "Operation complete.".to_string(),
        OperationPhase::Failed => "Operation failed.".to_string(),
        OperationPhase::Cancelled => "Operation cancelled.".to_string(),
    }
}

fn check_tty() -> io::Result<()> {
    let stdin_tty = io::stdin().is_terminal();
    let stdout_tty = io::stdout().is_terminal();
    info!(
        "Terminal check: stdin_tty={stdin_tty}, stdout_tty={stdout_tty}, euid={}",
        crate::tui::privilege::current_euid_or_0()
    );

    if !stdin_tty || !stdout_tty {
        error!(
            "stdin or stdout is not a TTY (stdin={stdin_tty}, stdout={stdout_tty}); \
             litho-tui must be run from an interactive terminal"
        );
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "stdin/stdout is not a terminal — run litho-tui from an interactive shell",
        ));
    }

    Ok(())
}

fn init_terminal() -> io::Result<TuiTerminal> {
    if let Err(e) = check_tty() {
        return Err(e);
    }

    if let Err(e) = enable_raw_mode() {
        error!("Failed to enable raw mode: {e:?}");
        return Err(e);
    }

    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
        error!("Failed to enter alternate screen: {e:?}");
        let _ = disable_raw_mode();
        return Err(e);
    }

    let backend = CrosstermBackend::new(stdout);
    match Terminal::new(backend) {
        Ok(terminal) => Ok(terminal),
        Err(e) => {
            error!("Failed to create ratatui terminal: {e:?}");
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            Err(e)
        }
    }
}

fn teardown_terminal(terminal: &mut TuiTerminal) {
    if let Err(e) = disable_raw_mode() {
        error!("Failed to disable raw mode during teardown: {e:?}");
    }
    if let Err(e) = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ) {
        error!("Failed to leave alternate screen during teardown: {e:?}");
    }
    if let Err(e) = terminal.show_cursor() {
        error!("Failed to show cursor during teardown: {e:?}");
    }
}

fn resume_terminal(terminal: &mut TuiTerminal) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.hide_cursor()?;
    Ok(())
}

fn restore_terminal(terminal: &mut TuiTerminal) -> io::Result<()> {
    if let Err(e) = disable_raw_mode() {
        error!("Failed to disable raw mode during shutdown: {e:?}");
        return Err(e);
    }
    if let Err(e) = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ) {
        error!("Failed to leave alternate screen during shutdown: {e:?}");
        return Err(e);
    }
    if let Err(e) = terminal.show_cursor() {
        error!("Failed to show cursor during shutdown: {e:?}");
        return Err(e);
    }
    Ok(())
}

pub async fn run_tui(launch: LaunchParams) -> Result<(), io::Error> {
    let mut terminal = init_terminal()?;
    let term_size = terminal.size()?;
    let log_mode = launch.mode.clone();
    let log_device = launch.device.clone();
    let log_image = launch.image.clone();
    let log_start = launch.start;

    let mut app = App::new(launch);
    info!(
        "TUI session: euid={}, root={}, polkit={}, terminal={}x{}, devices={}, \
         launch={{ mode={log_mode:?}, device={log_device:?}, image={log_image:?}, start={log_start} }}",
        crate::tui::privilege::current_euid_or_0(),
        app.is_root,
        app.polkit_available,
        term_size.width,
        term_size.height,
        app.devices.len(),
    );
    if app.auto_start_pending {
        app.start_operation();
    }
    let res = run_app(&mut terminal, &mut app).await;

    if let Err(err) = restore_terminal(&mut terminal) {
        if let Err(app_err) = res {
            error!("TUI event loop error: {app_err:?}");
        }
        return Err(err);
    }

    if let Err(err) = res {
        error!("TUI event loop error: {err:?}");
    }

    Ok(())
}

pub async fn run_app(terminal: &mut TuiTerminal, app: &mut App) -> io::Result<()> {
    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        if terminal_too_small(area) && !app.terminal_warn_logged {
            log::warn!(
                "Terminal below minimum ({}x{} < {}x{})",
                size.width,
                size.height,
                MIN_COLS,
                MIN_ROWS
            );
            app.terminal_warn_logged = true;
        }

        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if app.dialog != Dialog::None {
                        match app.dialog {
                            Dialog::NonRemovableConfirm { .. } => match key.code {
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
                            },
                            Dialog::ElevationConfirm => match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    app.confirm_elevation(true, terminal);
                                }
                                KeyCode::Char('n')
                                | KeyCode::Char('N')
                                | KeyCode::Esc
                                | KeyCode::Enter => {
                                    app.confirm_elevation(false, terminal);
                                }
                                _ => {}
                            },
                            Dialog::None => {}
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
                        KeyCode::Char('r') if app.focus == InputFocus::Device => {
                            app.refresh_devices();
                        }
                        KeyCode::Char('f') if app.focus == InputFocus::File => {
                            app.open_file_picker(terminal)?;
                        }
                        KeyCode::Char(' ') | KeyCode::Enter
                            if app.focus == InputFocus::Verify && !app.is_running =>
                        {
                            app.toggle_verify_checksum();
                        }
                        KeyCode::Enter => match app.focus {
                            InputFocus::Mode => {}
                            InputFocus::Device => app.open_device_picker(terminal)?,
                            InputFocus::File => app.open_file_picker(terminal)?,
                            InputFocus::Verify => {}
                            InputFocus::Start if !app.is_running => app.start_operation(),
                            InputFocus::Cancel if app.is_running => app.cancel_operation(),
                            _ => {}
                        },
                        KeyCode::Char('c') if app.is_running => app.cancel_operation(),
                        KeyCode::Esc if app.is_running => app.cancel_operation(),
                        _ => {}
                    }
                }
                Event::Resize(width, height) => {
                    terminal.resize(Rect::new(0, 0, width, height))?;
                }
                _ => {}
            }
        }

        app.check_progress();
    }
}
