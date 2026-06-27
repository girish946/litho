use crate::tui::app::Operation;
use liblitho::io_backend::{clone_io, flash_io, USES_SIMULATED_IO};
use liblitho::progress::{is_operation_cancelled, OperationPhase, OperationProgress};
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

        let on_progress = |progress: OperationProgress| {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let _ = tx.send(progress);
        };

        let cancel_ref = Some(cancel.as_ref());
        let result = match operation {
            Operation::Flash => flash_io(
                &image_path,
                &device_path,
                block_size,
                false,
                verify,
                Some(on_progress),
                cancel_ref,
            ),
            Operation::Clone => clone_io(
                &device_path,
                &image_path,
                block_size,
                false,
                Some(on_progress),
                cancel_ref,
            ),
        };

        match result {
            Err(error) if is_operation_cancelled(&error) => {
                let message = match operation {
                    Operation::Flash => {
                        "Flash cancelled — device may be partially written.".to_string()
                    }
                    Operation::Clone => {
                        "Clone cancelled — incomplete output file removed.".to_string()
                    }
                };
                info!("{op_name} cancelled");
                let _ = tx.send(
                    OperationProgress::new(OperationPhase::Cancelled).with_message(message),
                );
            }
            Err(error) => {
                let _ = tx.send(
                    OperationProgress::new(OperationPhase::Failed).with_message(error.to_string()),
                );
            }
            Ok(()) => {}
        }
    });
}