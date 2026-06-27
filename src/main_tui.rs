mod tui;

use clap::Parser;
use log::error;
use std::io;
use tui::launch::{LaunchParams, TuiCli};
use tui::logging::{init_logging, LogOptions};
use tui::run_tui;

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let cli = TuiCli::parse();

    init_logging(&LogOptions {
        log_file: cli.log_file.clone(),
        log_level: Some(cli.log_level.clone()),
    })
    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let launch: LaunchParams = cli.into();
    if let Err(err) = run_tui(launch).await {
        error!("litho-tui failed: {err:?}");
        return Err(err);
    }
    Ok(())
}
