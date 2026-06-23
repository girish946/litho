use crate::tui::app::Operation;
use liblitho::progress::{OperationPhase, OperationProgress};
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// Simulated flash/clone progress. Does not call `liblitho::flash` or `clone`.
pub fn spawn_operation(
    operation: Operation,
    device_path: String,
    image_path: String,
    cancel: Arc<AtomicBool>,
    tx: Sender<OperationProgress>,
) {
    tokio::task::spawn_blocking(move || {
        let op_name = match operation {
            Operation::Flash => "flash",
            Operation::Clone => "clone",
        };
        info!("Starting simulated {op_name}: device={device_path}, image={image_path}");

        let mut progress = 0.0f64;
        while progress < 100.0 {
            if cancel.load(Ordering::Relaxed) {
                info!("Simulated {op_name} cancelled");
                return;
            }
            std::thread::sleep(Duration::from_millis(180));
            progress = (progress + 8.0 + (progress as u64 % 5) as f64).min(100.0);
            let _ =
                tx.send(OperationProgress::new(OperationPhase::Writing).with_percentage(progress));
        }

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let _ = tx.send(
            OperationProgress::new(OperationPhase::Complete)
                .with_percentage(100.0)
                .with_message("Simulation complete".to_string()),
        );
    });
}
