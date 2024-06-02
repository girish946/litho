use clap::{Parser, Subcommand};
use liblitho::{clone, flash};
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
    println!("{percentage}%");
}

async fn callback_fn_async(tx: broadcast::Sender<String>, mut client: Client) {
    let mut rx = tx.subscribe();

    loop {
        let msg = match rx.recv().await {
            Ok(msg) => msg,
            Err(e) => {
                println!("Error: {}", e);
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
                println!("Published");
            }
            Err(e) => {
                println!("Error: {}", e);
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
            println!(
                "file: {}, device: {}, block_size: {:?}, silent: {:?}",
                file, device, block_size, silent
            );
            let blk_size = block_size.unwrap_or(4096);
            let silent = silent.unwrap_or(false);

            if let Some(sockfile) = sockfile {
                println!("sockfile: {}", sockfile);
                let (tx, _rx) = broadcast::channel::<String>(1000);

                let client_type = simple_pub_sub::client::PubSubUnixClient { path: sockfile };
                let mut client_obj = simple_pub_sub::client::Client::new(
                    simple_pub_sub::client::PubSubClient::Unix(client_type),
                );

                // connect to the server.
                let _ = client_obj.connect().await;

                tokio::spawn(callback_fn_async(tx.clone(), client_obj));

                match clone(file, device, blk_size, silent, Some(callback_fn), Some(tx)) {
                    Ok(()) => println!("Success"),
                    Err(e) => println!("Error: {}", e),
                };
            } else {
                match clone(device, file, blk_size, silent, Some(callback_fn), None) {
                    Ok(()) => println!("Success"),
                    Err(e) => println!("Error: {}", e),
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
            println!(
                "file: {}, device: {}, block_size: {:?}, silent: {:?}",
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
                let _ = client_obj.connect().await;

                let (tx, _rx) = broadcast::channel(1000);
                tokio::spawn(callback_fn_async(tx.clone(), client_obj));
                match flash(file, device, blk_size, silent, Some(callback_fn), Some(tx)) {
                    Ok(_) => println!("Success"),
                    Err(e) => println!("Error: {}", e),
                };
            } else {
                match flash(file, device, blk_size, silent, Some(callback_fn), None) {
                    Ok(_) => println!("Success"),
                    Err(e) => println!("Error: {}", e),
                };
            }
        }
        Commands::Query { device } => match device {
            Some(device) => {
                println!("device: {}", device);
            }
            None => {
                println!("No device specified");
                match liblitho::devices::get_storage_devices() {
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
