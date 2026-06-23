use clap::{Parser, Subcommand};
use liblitho::progress::OperationProgress;
use liblitho::{clone, flash};
use log::{error, info};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Clone {
        /// file to which device should be cloned.
        #[clap(short, long)]
        file: String,

        /// device
        #[clap(short, long)]
        device: String,

        /// block size
        #[clap(short, long)]
        block_size: Option<usize>,

        /// suppress progress output
        #[clap(short, long)]
        silent: Option<bool>,
    },
    Flash {
        /// file to be written to the device
        #[clap(short, long)]
        file: String,

        /// device
        #[clap(short, long)]
        device: String,

        /// block size
        #[clap(short, long)]
        block_size: Option<usize>,

        /// suppress progress output
        #[clap(short, long)]
        silent: Option<bool>,
    },
    Query {
        /// device
        #[clap(short, long)]
        device: Option<String>,
    },
}

fn log_progress(p: OperationProgress) {
    match (p.percentage, p.message) {
        (Some(pct), _) => info!("{:?}: {:.1}%", p.phase, pct),
        (None, Some(msg)) => info!("{:?}: {} ({})", p.phase, p.bytes_processed, msg),
        (None, None) => info!("{:?}: {} bytes", p.phase, p.bytes_processed),
    }
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Clone {
            file,
            device,
            block_size,
            silent,
        } => {
            info!(
                "Clone command: file={}, device={}, block_size={:?}, silent={:?}",
                file, device, block_size, silent
            );
            let blk_size = block_size.unwrap_or(4096);
            let silent = silent.unwrap_or(false);

            match clone(
                device,
                file,
                blk_size,
                silent,
                if silent {
                    None
                } else {
                    Some(log_progress)
                },
            ) {
                Ok(()) => info!("Clone operation completed successfully"),
                Err(e) => error!("Clone operation failed: {}", e),
            };
        }
        Commands::Flash {
            file,
            device,
            block_size,
            silent,
        } => {
            info!(
                "Flash command: file={}, device={}, block_size={:?}, silent={:?}",
                file, device, block_size, silent
            );
            let blk_size = block_size.unwrap_or(4096);
            let silent = silent.unwrap_or(false);

            match flash(
                file,
                device,
                blk_size,
                silent,
                if silent {
                    None
                } else {
                    Some(log_progress)
                },
            ) {
                Ok(_) => info!("Flash operation completed successfully"),
                Err(e) => error!("Flash operation failed: {}", e),
            }
        }
        Commands::Query { device } => match device {
            Some(device) => {
                info!("Querying device: {}", device);
            }
            None => {
                info!("Querying all storage devices");
                match liblitho::devices::get_storage_devices() {
                    Ok(devices) => {
                        for device in devices {
                            info!("{}", device);
                        }
                    }
                    Err(e) => {
                        error!("Failed to get storage devices: {}", e)
                    }
                };
            }
        },
    }
}