use std::fs;
use std::process::{Command, Stdio};

pub fn is_running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub fn pkexec_on_path() -> bool {
    Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

fn extract_executable_path_from_ps(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        if token.starts_with('/') && token.contains("polkit") {
            return Some(token.to_string());
        }
    }
    None
}

/// Detect a polkit authentication agent (running process or known install path).
pub fn find_polkit_auth_agent() -> Option<String> {
    let user = current_username();

    if let Ok(output) = Command::new("ps")
        .args(["-u", &user, "-o", "pid,comm,args", "--no-headers"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let lower = line.to_lowercase();
            if lower.contains("polkit")
                && (lower.contains("agent") || lower.contains("-authentication-agent"))
            {
                if let Some(path) = extract_executable_path_from_ps(line) {
                    if fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
                        return Some(path);
                    }
                }
            }
        }
    }

    const CANDIDATES: &[&str] = &[
        "/usr/libexec/polkit-gnome-authentication-agent-1",
        "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
        "/usr/libexec/polkit-kde-authentication-agent-1",
        "/usr/lib/polkit-kde-authentication-agent-1",
        "/usr/lib/x86_64-linux-gnu/libexec/polkit-kde-authentication-agent-1",
        "/usr/libexec/xfce-polkit",
        "/usr/libexec/polkit-mate-authentication-agent-1",
        "/usr/bin/lxpolkit",
        "/usr/libexec/cinnamon-polkit",
    ];

    for path in CANDIDATES {
        if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
            return Some(path.to_string());
        }
    }

    None
}

/// True when `pkexec` exists and a polkit auth agent is likely available.
pub fn polkit_agent_available() -> bool {
    pkexec_on_path() && find_polkit_auth_agent().is_some()
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

    let exe =
        std::env::current_exe().map_err(|e| format!("Could not resolve executable: {}", e))?;
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
    let exe =
        std::env::current_exe().map_err(|e| format!("Could not resolve executable: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polkit_available_implies_pkexec_on_path() {
        if polkit_agent_available() {
            assert!(pkexec_on_path());
        }
    }
}