use clap::{Parser, Subcommand};
use litho::write_image;
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
            let _ = match write_image(file, device, blk_size) {
                Ok(_) => println!("Success"),
                Err(e) => println!("Error: {}", e),
            };
        }
    }
}
