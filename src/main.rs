mod cli_output;

use clap::{Parser, Subcommand};
use liblitho::io_backend::{clone_io, flash_io};
use cli_output::{CliOutput, OutputMode};
use std::process::ExitCode;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Output style: terminal progress bar or GUI-friendly line protocol.
    #[arg(short = 'o', long = "output-mode", value_enum, default_value_t = OutputMode::Terminal, global = true)]
    output_mode: OutputMode,

    /// Validate inputs and print the operation that would run, without performing I/O.
    #[arg(long = "dry-run", global = true, default_value_t = false)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Read a block device into an image file.
    Clone {
        /// Output image file.
        #[arg(short, long)]
        file: String,

        /// Source block device.
        #[arg(short, long)]
        device: String,

        /// I/O buffer size in bytes.
        #[arg(short, long, default_value_t = 4096)]
        block_size: usize,

        /// Suppress progress output.
        #[arg(short, long, default_value_t = false)]
        silent: bool,
    },
    /// Write an image file to a block device.
    Flash {
        /// Image file to write.
        #[arg(short, long)]
        file: String,

        /// Target block device.
        #[arg(short, long)]
        device: String,

        /// I/O buffer size in bytes.
        #[arg(short, long, default_value_t = 4096)]
        block_size: usize,

        /// Suppress progress output.
        #[arg(short, long, default_value_t = false)]
        silent: bool,

        /// After writing, read the device back and compare SHA-256 checksums.
        #[arg(long = "verify", default_value_t = false)]
        verify: bool,
    },
    /// List storage devices or query one device.
    Query {
        /// Optional device path to query.
        #[arg(short, long)]
        device: Option<String>,
    },
}

fn run(cli: Cli) -> ExitCode {
    let mut out = CliOutput::new(cli.output_mode);

    match cli.command {
        Commands::Clone {
            file,
            device,
            block_size,
            silent,
        } => run_clone(&mut out, &device, &file, block_size, silent, cli.dry_run),
        Commands::Flash {
            file,
            device,
            block_size,
            silent,
            verify,
        } => run_flash(&mut out, &file, &device, block_size, silent, verify, cli.dry_run),
        Commands::Query { device } => run_query(&out, device.as_deref()),
    }
}

fn run_flash(
    out: &mut CliOutput,
    file: &str,
    device: &str,
    block_size: usize,
    silent: bool,
    verify: bool,
    dry_run: bool,
) -> ExitCode {
    if let Err(e) = liblitho::devices::validate_device_safe_for_io(device) {
        out.error(&e);
        return ExitCode::FAILURE;
    }

    if dry_run {
        out.dry_run_ok("flash", file, device, block_size);
        return ExitCode::SUCCESS;
    }

    out.operation_start("Flashing", file, device, block_size);

    let result = if silent {
        flash_io::<fn(liblitho::progress::OperationProgress)>(
            file, device, block_size, true, verify, None,
        )
    } else {
        flash_io(
            file,
            device,
            block_size,
            false,
            verify,
            Some(|event| {
                out.on_progress(&event);
            }),
        )
    };

    out.finish_progress_line();

    match result {
        Ok(()) => {
            out.done_ok("flash");
            ExitCode::SUCCESS
        }
        Err(e) => {
            out.error(&e.to_string());
            ExitCode::FAILURE
        }
    }
}

fn run_clone(
    out: &mut CliOutput,
    device: &str,
    file: &str,
    block_size: usize,
    silent: bool,
    dry_run: bool,
) -> ExitCode {
    if let Err(e) = liblitho::devices::validate_device_safe_for_io(device) {
        out.error(&e);
        return ExitCode::FAILURE;
    }

    if dry_run {
        out.dry_run_ok("clone", device, file, block_size);
        return ExitCode::SUCCESS;
    }

    out.operation_start("Cloning", device, file, block_size);

    let result = if silent {
        clone_io::<fn(liblitho::progress::OperationProgress)>(
            device, file, block_size, true, None,
        )
    } else {
        clone_io(
            device,
            file,
            block_size,
            false,
            Some(|event| {
                out.on_progress(&event);
            }),
        )
    };

    out.finish_progress_line();

    match result {
        Ok(()) => {
            out.done_ok("clone");
            ExitCode::SUCCESS
        }
        Err(e) => {
            out.error(&e.to_string());
            ExitCode::FAILURE
        }
    }
}

fn run_query(out: &CliOutput, device: Option<&str>) -> ExitCode {
    match device {
        Some(path) => {
            out.query_status(&format!("Querying device: {path}"));
            // Single-device lookup is not implemented yet; list all and let the user filter.
            match liblitho::devices::get_storage_devices() {
                Ok(devices) => {
                    let mut found = false;
                    for dev in devices {
                        if device_path_matches(&dev.device_name, path) {
                            out.query_device(&dev);
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        out.error(&format!("Device not found: {path}"));
                        return ExitCode::FAILURE;
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    out.error(&e.to_string());
                    ExitCode::FAILURE
                }
            }
        }
        None => match liblitho::devices::get_storage_devices() {
            Ok(devices) => {
                if devices.is_empty() {
                    out.query_status("No storage devices found");
                } else {
                    for dev in devices {
                        out.query_device(&dev);
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                out.error(&e.to_string());
                ExitCode::FAILURE
            }
        },
    }
}

fn device_path_matches(device_name: &str, query_path: &str) -> bool {
    query_path.trim() == device_name.trim()
}

fn main() -> ExitCode {
    run(Cli::parse())
}
