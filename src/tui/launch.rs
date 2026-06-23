use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "litho-tui", about = "Interactive terminal UI for litho")]
pub struct TuiCli {
    /// flash or clone
    #[arg(short, long)]
    pub mode: Option<String>,

    /// target block device (e.g. /dev/sdb)
    #[arg(short, long)]
    pub device: Option<String>,

    /// image file path (flash source or clone output)
    #[arg(short, long)]
    pub image: Option<String>,

    /// alias for --image
    #[arg(short, long)]
    pub file: Option<String>,

    /// begin operation immediately (never set by pkexec re-launch)
    #[arg(long)]
    pub start: bool,

    /// log file path (default: ~/.cache/litho/litho-tui.log)
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// log level: error, warn, info, debug, trace
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchParams {
    pub mode: Option<String>,
    pub device: Option<String>,
    pub image: Option<String>,
    pub start: bool,
}

impl From<TuiCli> for LaunchParams {
    fn from(cli: TuiCli) -> Self {
        let mode = cli.mode.map(|m| normalize_mode(&m));
        let image = cli.image.or(cli.file);
        LaunchParams {
            mode,
            device: cli.device,
            image,
            start: cli.start,
        }
    }
}

pub fn normalize_mode(mode: &str) -> String {
    let lower = mode.to_lowercase();
    if lower == "clone" || lower == "backup" {
        "clone".to_string()
    } else {
        "flash".to_string()
    }
}

/// True when launch args pre-fill the form and the UI should focus Start (unless `--start`).
pub fn launch_prefilled(launch: &LaunchParams, image_file_nonempty: bool) -> bool {
    launch.mode.is_some() || launch.device.is_some() || image_file_nonempty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_mode_maps_backup_to_clone() {
        assert_eq!(normalize_mode("backup"), "clone");
        assert_eq!(normalize_mode("CLONE"), "clone");
        assert_eq!(normalize_mode("flash"), "flash");
    }

    #[test]
    fn launch_prefilled_detects_any_launch_arg() {
        assert!(launch_prefilled(
            &LaunchParams {
                mode: Some("flash".into()),
                ..Default::default()
            },
            false
        ));
        assert!(launch_prefilled(
            &LaunchParams {
                device: Some("/dev/sdb".into()),
                ..Default::default()
            },
            false
        ));
        assert!(launch_prefilled(&LaunchParams::default(), true));
        assert!(!launch_prefilled(&LaunchParams::default(), false));
    }
}
