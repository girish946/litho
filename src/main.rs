use clap::{Parser, Subcommand};
use liblitho::{clone, flash};
use log::{error, info};
use simple_pub_sub::client::Client;
use tokio::sync::broadcast;

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

        /// sockfile
        #[clap(short = 'F', long)]
        sockfile: Option<String>,
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

        /// sockfile
        #[clap(short = 'F', long)]
        sockfile: Option<String>,
    },
    Query {
        /// device
        #[clap(short, long)]
        device: Option<String>,
    },
}

fn callback_fn(percentage: f64) {
    info!("Progress: {}%", percentage);
}

async fn callback_fn_async(tx: broadcast::Sender<String>, mut client: Client) {
    let mut rx = tx.subscribe();

    loop {
        let msg = match rx.recv().await {
            Ok(msg) => msg,
            Err(e) => {
                error!("Failed to receive message: {}", e);
                continue;
            }
        };
        match &client
            .publish(
                "test".to_string(),
                format!("{{ \"bytes_cloned\": {} }}", msg)
                    .as_bytes()
                    .to_vec(),
            )
            .await
        {
            Ok(_) => {
                info!("Published progress update");
            }
            Err(e) => {
                error!("Failed to publish: {}", e);
            }
        };
    }
}

#[tokio::main]
pub async fn main() {
    env_logger::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Clone {
            file,
            device,
            block_size,
            silent,
            sockfile,
        } => {
            info!(
                "Clone command: file={}, device={}, block_size={:?}, silent={:?}",
                file, device, block_size, silent
            );
            let blk_size = block_size.unwrap_or(4096);
            let silent = silent.unwrap_or(false);

            if let Some(sockfile) = sockfile {
                info!("Using socket file: {}", sockfile);
                let (tx, _rx) = broadcast::channel::<String>(1000);

                let client_type = simple_pub_sub::client::PubSubUnixClient { path: sockfile };
                let mut client_obj = simple_pub_sub::client::Client::new(
                    simple_pub_sub::client::PubSubClient::Unix(client_type),
                );

                // connect to the server.
                if let Err(e) = client_obj.connect().await {
                    error!("Failed to connect to pub-sub server: {}", e);
                }

                tokio::spawn(callback_fn_async(tx.clone(), client_obj));

                match clone(file, device, blk_size, silent, Some(callback_fn), Some(tx)) {
                    Ok(()) => info!("Clone operation completed successfully"),
                    Err(e) => error!("Clone operation failed: {}", e),
                };
            } else {
                match clone(device, file, blk_size, silent, Some(callback_fn), None) {
                    Ok(()) => info!("Clone operation completed successfully"),
                    Err(e) => error!("Clone operation failed: {}", e),
                };
            }
        }
        Commands::Flash {
            file,
            device,
            block_size,
            silent,
            sockfile,
        } => {
            info!(
                "Flash command: file={}, device={}, block_size={:?}, silent={:?}",
                file, device, block_size, silent
            );
            let blk_size = block_size.unwrap_or(4096);
            let silent = silent.unwrap_or(false);
            if let Some(sockfile) = sockfile {
                let client_type = simple_pub_sub::client::PubSubUnixClient { path: sockfile };
                let mut client_obj = simple_pub_sub::client::Client::new(
                    simple_pub_sub::client::PubSubClient::Unix(client_type),
                );

                // connect to the server.
                if let Err(e) = client_obj.connect().await {
                    error!("Failed to connect to pub-sub server: {}", e);
                }

                let (tx, _rx) = broadcast::channel(1000);
                tokio::spawn(callback_fn_async(tx.clone(), client_obj));
                match flash(file, device, blk_size, silent, Some(callback_fn), Some(tx)) {
                    Ok(_) => info!("Flash operation completed successfully"),
                    Err(e) => error!("Flash operation failed: {}", e),
                };
            } else {
                match flash(file, device, blk_size, silent, Some(callback_fn), None) {
                    Ok(_) => info!("Flash operation completed successfully"),
                    Err(e) => error!("Flash operation failed: {}", e),
                };
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
