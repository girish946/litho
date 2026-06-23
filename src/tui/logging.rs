use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

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

pub fn init_logging(opts: &LogOptions) -> Result<(), String> {
    let level = opts
        .log_level
        .as_deref()
        .unwrap_or("info")
        .to_ascii_lowercase();

    let log_path = opts.log_file.clone().unwrap_or_else(default_log_path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create log directory {}: {}", parent.display(), e))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file {}: {}", log_path.display(), e))?;

    writeln!(file, "--- litho-tui session ---")
        .map_err(|e| format!("Failed to write log header: {}", e))?;

    env_logger::Builder::new()
        .filter_level(match level.as_str() {
            "error" => log::LevelFilter::Error,
            "warn" | "warning" => log::LevelFilter::Warn,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        })
        .target(env_logger::Target::Pipe(Box::new(file)))
        .format_timestamp_secs()
        .init();

    log::info!("Logging to {}", log_path.display());
    Ok(())
}
