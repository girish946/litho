use crate::tui::app::Operation;
use liblitho::io_backend::{clone_io, flash_io, USES_SIMULATED_IO};
use liblitho::progress::{OperationPhase, OperationProgress};
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub fn spawn_operation(
    operation: Operation,
    device_path: String,
    image_path: String,
    block_size: usize,
    verify: bool,
    cancel: Arc<AtomicBool>,
    tx: Sender<OperationProgress>,
) {
    tokio::task::spawn_blocking(move || {
        let op_name = match operation {
            Operation::Flash => "flash",
            Operation::Clone => "clone",
        };

        if USES_SIMULATED_IO {
            info!(
                "Starting simulated {op_name}: device={device_path}, image={image_path}, block_size={block_size}"
            );
        } else {
            info!(
                "Starting {op_name}: device={device_path}, image={image_path}, block_size={block_size}"
            );
        }

        let result = match operation {
            Operation::Flash => {
                let on_progress = |progress: OperationProgress| {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = tx.send(progress);
                };
                flash_io(
                    &image_path,
                    &device_path,
                    block_size,
                    false,
                    verify,
                    Some(on_progress),
                )
            }
            Operation::Clone => {
                let on_progress = |progress: OperationProgress| {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = tx.send(progress);
                };
                clone_io(
                    &device_path,
                    &image_path,
                    block_size,
                    false,
                    Some(on_progress),
                )
            }
        };

        if cancel.load(Ordering::Relaxed) {
            info!("{op_name} cancelled");
            return;
        }

        if let Err(error) = result {
            let _ = tx.send(
                OperationProgress::new(OperationPhase::Failed).with_message(error.to_string()),
            );
        }
    });
}