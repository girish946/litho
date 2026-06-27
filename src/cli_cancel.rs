//! Cooperative cancel for litho CLI subprocesses (e.g. Lithographer sidecar).
//!
//! - **Cancel file** — primary for pkexec (root child reads user cache file).
//! - **Stdin** — secondary when stdin is a pipe and forwarded.
//! - **SIGTERM / SIGINT** — fallback at the same privilege level.

use liblitho::cancel::cancel_requested_in_file;
use liblitho::progress::is_stdin_cancel_line;
use std::io::{self, BufRead, IsTerminal};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

static OPERATION_CANCEL: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static HANDLERS_INSTALLED: std::sync::Once = std::sync::Once::new();

const CANCEL_FILE_POLL_MS: u64 = 50;

extern "C" fn on_cancel_signal(_: libc::c_int) {
    if let Some(flag) = OPERATION_CANCEL.get() {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Reset and return the cancel flag for one flash/clone invocation.
pub fn prepare_operation_cancel() -> Arc<AtomicBool> {
    let flag = OPERATION_CANCEL
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone();
    flag.store(false, Ordering::Relaxed);
    HANDLERS_INSTALLED.call_once(|| {
        #[cfg(unix)]
        unsafe {
            install_handler(libc::SIGTERM);
            install_handler(libc::SIGINT);
        }
    });
    flag
}

/// Start background listeners for cancel requests from GUI hosts.
pub fn spawn_cancel_watchers(cancel: Arc<AtomicBool>, cancel_file: Option<PathBuf>) {
    if let Some(path) = cancel_file {
        spawn_cancel_file_watcher(cancel.clone(), path);
    }
    spawn_stdin_cancel_listener(cancel);
}

fn spawn_cancel_file_watcher(cancel: Arc<AtomicBool>, path: PathBuf) {
    thread::spawn(move || {
        while !cancel.load(Ordering::Relaxed) {
            if cancel_requested_in_file(&path) {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
            thread::sleep(Duration::from_millis(CANCEL_FILE_POLL_MS));
        }
    });
}

/// Listen on stdin for `cancel` when stdin is a pipe.
fn spawn_stdin_cancel_listener(cancel: Arc<AtomicBool>) {
    if io::stdin().is_terminal() {
        return;
    }

    thread::spawn(move || {
        let reader = io::BufReader::new(io::stdin());
        for line in reader.lines().map_while(Result::ok) {
            if is_stdin_cancel_line(&line) {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
        }
    });
}

/// Exit code when an operation stops due to user cancellation.
pub const CANCEL_EXIT_CODE: u8 = 3;

#[cfg(unix)]
unsafe fn install_handler(sig: libc::c_int) {
    let mut action: libc::sigaction = std::mem::zeroed();
    action.sa_sigaction = on_cancel_signal as usize;
    action.sa_flags = 0;
    libc::sigemptyset(&mut action.sa_mask);
    libc::sigaction(sig, &action, std::ptr::null_mut());
}

#[cfg(test)]
mod tests {
    use super::*;
    use liblitho::cancel::{cancel_requested_in_file, init_cancel_file, request_cancel_via_file};
    use liblitho::progress::STDIN_CANCEL_LINE;
    use std::path::PathBuf;

    #[test]
    fn cancel_file_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "litho-cancel-test-{}-{}.flag",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        init_cancel_file(&path).unwrap();
        assert!(!cancel_requested_in_file(&path));
        request_cancel_via_file(&path).unwrap();
        assert!(cancel_requested_in_file(&path));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stdin_cancel_line_matches_case_insensitive() {
        use liblitho::progress::is_stdin_cancel_line;
        assert!(is_stdin_cancel_line(STDIN_CANCEL_LINE));
        assert!(is_stdin_cancel_line("CANCEL\n"));
        assert!(!is_stdin_cancel_line("stop"));
    }
}