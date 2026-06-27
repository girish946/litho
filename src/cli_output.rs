use liblitho::progress::{OperationPhase, OperationProgress};
use std::io::{stdout, IsTerminal, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputMode {
    /// Human-readable terminal output with an in-place progress bar.
    Terminal,
    /// One structured line per event for GUI hosts (e.g. Lithographer).
    Gui,
}

const BAR_WIDTH: usize = 40;

pub struct CliOutput {
    mode: OutputMode,
    is_tty: bool,
}

impl CliOutput {
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            is_tty: stdout().is_terminal(),
        }
    }

    pub fn operation_start(&self, verb: &str, source: &str, target: &str, block_size: usize) {
        match self.mode {
            OutputMode::Terminal => {
                println!("{verb} {source} → {target} (block_size={block_size})");
            }
            OutputMode::Gui => {
                println!(
                    "@status phase=preparing msg={}",
                    quote_gui(format!(
                        "{verb} {source} → {target} (block_size={block_size})"
                    ))
                );
            }
        }
    }

    pub fn on_progress(&mut self, progress: &OperationProgress) {
        if self.mode == OutputMode::Gui {
            print_gui_progress(progress);
            return;
        }

        if let Some(pct) = progress.percentage {
            let bar = progress_bar(pct, BAR_WIDTH);
            let phase = phase_label(progress.phase);
            let line = format!("{phase:<14} [{bar}] {pct:5.1}%");
            if self.is_tty {
                print!("\r{line}");
                let _ = stdout().flush();
            } else {
                println!("{line}");
            }
            return;
        }

        let phase = phase_label(progress.phase);
        if let Some(ref msg) = progress.message {
            println!("{phase}: {msg} ({})", progress.bytes_processed);
        } else {
            println!("{phase}: {} bytes", progress.bytes_processed);
        }
    }

    pub fn finish_progress_line(&mut self) {
        if self.mode == OutputMode::Terminal && self.is_tty {
            println!();
        }
    }

    pub fn done_ok(&self, operation: &str) {
        match self.mode {
            OutputMode::Terminal => {
                println!("Done: {operation} completed successfully");
            }
            OutputMode::Gui => {
                println!("@done ok");
            }
        }
    }

    pub fn dry_run_ok(&self, operation: &str, source: &str, target: &str, block_size: usize) {
        match self.mode {
            OutputMode::Terminal => {
                println!(
                    "Dry run: would {operation} {source} → {target} (block_size={block_size})"
                );
            }
            OutputMode::Gui => {
                println!(
                    "@status phase=preparing msg={}",
                    quote_gui(format!(
                        "Dry run: would {operation} {source} → {target} (block_size={block_size})"
                    ))
                );
                println!("@done ok");
            }
        }
    }

    pub fn error(&self, msg: &str) {
        match self.mode {
            OutputMode::Terminal => {
                eprintln!("Error: {msg}");
            }
            OutputMode::Gui => {
                eprintln!("@error msg={}", quote_gui(msg));
            }
        }
    }

    pub fn query_device(&self, device: &liblitho::devices::DeviceInfo) {
        match self.mode {
            OutputMode::Terminal => println!("{device}"),
            OutputMode::Gui => println!("{}", format_device_line(device)),
        }
    }

    pub fn query_status(&self, msg: &str) {
        match self.mode {
            OutputMode::Terminal => println!("{msg}"),
            OutputMode::Gui => println!("@status msg={}", quote_gui(msg)),
        }
    }
}

fn phase_label(phase: OperationPhase) -> &'static str {
    match phase {
        OperationPhase::Preparing => "Preparing",
        OperationPhase::Decompressing => "Decompressing",
        OperationPhase::Writing => "Writing",
        OperationPhase::Verifying => "Verifying",
        OperationPhase::Complete => "Complete",
        OperationPhase::Failed => "Failed",
        OperationPhase::Cancelled => "Cancelled",
    }
}

fn progress_bar(percentage: f64, width: usize) -> String {
    let pct = percentage.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width.saturating_sub(filled);
    format!("{}{}", "=".repeat(filled), "-".repeat(empty))
}

fn quote_gui(value: impl AsRef<str>) -> String {
    let escaped = value.as_ref().replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn print_gui_progress(progress: &OperationProgress) {
    let phase = phase_snake(progress.phase);
    let mut parts = vec![format!("@progress phase={phase}")];

    if let Some(pct) = progress.percentage {
        parts.push(format!("pct={pct:.1}"));
    }
    parts.push(format!("bytes={}", progress.bytes_processed));
    if let Some(total) = progress.bytes_total {
        parts.push(format!("total={total}"));
    }
    if let Some(ref msg) = progress.message {
        parts.push(format!("msg={}", quote_gui(msg)));
    }

    println!("{}", parts.join(" "));
}

fn phase_snake(phase: OperationPhase) -> &'static str {
    match phase {
        OperationPhase::Preparing => "preparing",
        OperationPhase::Decompressing => "decompressing",
        OperationPhase::Writing => "writing",
        OperationPhase::Verifying => "verifying",
        OperationPhase::Complete => "complete",
        OperationPhase::Failed => "failed",
        OperationPhase::Cancelled => "cancelled",
    }
}

fn format_device_line(device: &liblitho::devices::DeviceInfo) -> String {
    format!(
        "@device name={} vendor={} model={} removable={} size={}",
        quote_gui(&device.device_name),
        quote_gui(&device.vendor_name),
        quote_gui(&device.model_name),
        device.removable,
        device.size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_half() {
        assert_eq!(progress_bar(50.0, 10), "=====-----");
    }

    #[test]
    fn progress_bar_full() {
        assert_eq!(progress_bar(100.0, 8), "========");
    }

    #[test]
    fn quote_gui_escapes() {
        assert_eq!(quote_gui(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn phase_snake_labels() {
        assert_eq!(phase_snake(OperationPhase::Writing), "writing");
    }
}
