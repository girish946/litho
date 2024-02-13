use clap::{Parser, Subcommand};
use litho::{clone, flash};

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

        /// message to be published
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

        /// message to be published
        #[clap(short, long)]
        silent: Option<bool>,
    },
    Query {
        /// device
        #[clap(short, long)]
        device: Option<String>,
    },
}

fn callback_fn(percentage: f64) {
    println!("{percentage}%");
}

#[tokio::main]
pub async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Clone {
            file,
            device,
            block_size,
            silent,
        } => {
            println!(
                "file: {}, device: {}, block_size: {:?}, silent: {:?}",
                file, device, block_size, silent
            );
            let blk_size = match block_size {
                Some(size) => size,
                None => 4096,
            };
            let silent = match silent {
                Some(silent) => silent,
                None => false,
            };
            let _ = match clone(device, file, blk_size, silent, callback_fn) {
                Ok(_) => println!("Success"),
                Err(e) => println!("Error: {}", e),
            };
        }
        Commands::Flash {
            file,
            device,
            block_size,
            silent,
        } => {
            println!(
                "file: {}, device: {}, block_size: {:?}, silent: {:?}",
                file, device, block_size, silent
            );
            let blk_size = match block_size {
                Some(size) => size,
                None => 4096,
            };
            let silent = match silent {
                Some(silent) => silent,
                None => false,
            };
            let _ = match flash(file, device, blk_size, silent, callback_fn) {
                Ok(_) => println!("Success"),
                Err(e) => println!("Error: {}", e),
            };
        }
        Commands::Query { device } => match device {
            Some(device) => {
                println!("device: {}", device);
            }
            None => {
                println!("No device specified");
                match litho::devices::get_storage_devices() {
                    Ok(devices) => {
                        for device in devices {
                            println!("{}", device);
                        }
                    }
                    Err(e) => {
                        println!("Error: {}", e)
                    }
                };
            }
        },
    }
}
