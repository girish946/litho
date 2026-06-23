use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Maximum log file size before rotation (5 MiB).
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LogOptions {
    pub log_file: Option<PathBuf>,
    pub log_level: Option<String>,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            log_file: None,
            log_level: None,
        }
    }
}

fn default_log_path() -> PathBuf {
    dirs_fallback().join("litho").join("litho-tui.log")
}

fn dirs_fallback() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn stderr_logging_requested(opts: &LogOptions) -> bool {
    let env = std::env::var_os("LITHO_LOG_STDERR");
    let env_on = env.as_ref().is_some_and(|v| {
        let s = v.to_string_lossy();
        !s.is_empty() && s != "0" && s != "false"
    });
    let file_is_stderr = opts
        .log_file
        .as_ref()
        .is_some_and(|p| p.as_os_str() == OsStr::new("-"));
    env_on || file_is_stderr
}

fn parse_level(level: &str) -> log::LevelFilter {
    match level.to_ascii_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" | "warning" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

fn rotate_if_oversized(path: &Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("Failed to stat log file: {e}"))?;
    if meta.len() <= MAX_LOG_BYTES {
        return Ok(());
    }

    let backup = path.with_extension("log.old");
    let _ = fs::remove_file(&backup);
    fs::rename(path, &backup)
        .map_err(|e| format!("Failed to rotate log file {}: {e}", path.display()))?;
    Ok(())
}

pub fn init_logging(opts: &LogOptions) -> Result<(), String> {
    let level = opts
        .log_level
        .as_deref()
        .unwrap_or("info")
        .to_ascii_lowercase();
    let filter = parse_level(&level);

    if stderr_logging_requested(opts) {
        env_logger::Builder::new()
            .filter_level(filter)
            .target(env_logger::Target::Stderr)
            .format_timestamp_secs()
            .init();
        log::info!("Logging to stderr (debug mode)");
        return Ok(());
    }

    let log_path = opts.log_file.clone().unwrap_or_else(default_log_path);

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create log directory {}: {}", parent.display(), e))?;
    }

    if log_path.exists() {
        rotate_if_oversized(&log_path)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file {}: {}", log_path.display(), e))?;

    writeln!(file, "--- litho-tui session ---")
        .map_err(|e| format!("Failed to write log header: {e}"))?;

    env_logger::Builder::new()
        .filter_level(filter)
        .target(env_logger::Target::Pipe(Box::new(file)))
        .format_timestamp_secs()
        .init();

    log::info!("Logging to {}", log_path.display());
    Ok(())
}