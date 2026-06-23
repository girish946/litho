use std::process::{Command, Stdio};

pub fn is_running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub fn polkit_agent_available() -> bool {
    Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn preserve_terminal_env(cmd: &mut Command) {
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    for var in ["TERM", "COLORTERM", "LANG", "LC_ALL", "LC_CTYPE"] {
        if let Ok(value) = std::env::var(var) {
            cmd.env(var, value);
        }
    }
}

fn elevated_args(mode: &str, device: &str, image: &str) -> [String; 6] {
    [
        "--mode".to_string(),
        mode.to_string(),
        "--device".to_string(),
        device.to_string(),
        "--image".to_string(),
        image.to_string(),
    ]
}

/// Replace the current process with an elevated litho-tui instance.
///
/// Never passes `--start`. On success this function does not return.
#[cfg(unix)]
pub fn relaunch_elevated(mode: &str, device: &str, image: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe().map_err(|e| format!("Could not resolve executable: {}", e))?;
    let args = elevated_args(mode, device, image);

    let mut cmd = if is_running_as_root() {
        let mut cmd = Command::new(&exe);
        cmd.args(&args);
        cmd
    } else {
        let mut cmd = Command::new("pkexec");
        cmd.arg(&exe).args(&args);
        cmd
    };

    preserve_terminal_env(&mut cmd);

    let e = cmd.exec();
    if is_running_as_root() {
        Err(format!("Failed to exec elevated litho-tui: {e}"))
    } else {
        Err(format!(
            "Failed to exec pkexec: {e}. Is pkexec installed and is a polkit agent running?"
        ))
    }
}

/// Non-Unix fallback: spawn and let the caller exit.
#[cfg(not(unix))]
pub fn relaunch_elevated(mode: &str, device: &str, image: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not resolve executable: {}", e))?;
    let args = elevated_args(mode, device, image);

    let mut cmd = if is_running_as_root() {
        let mut cmd = Command::new(&exe);
        cmd.args(&args);
        cmd
    } else {
        let mut cmd = Command::new("pkexec");
        cmd.arg(&exe).args(&args);
        cmd
    };

    preserve_terminal_env(&mut cmd);

    cmd.spawn().map_err(|e| {
        format!(
            "Failed to spawn elevated litho-tui: {}. Is pkexec installed and is a polkit agent running?",
            e
        )
    })?;

    Ok(())
}