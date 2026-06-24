use anyhow::Result;
use crate::progress::{OperationPhase, OperationProgress};
use std::thread;
use std::time::Duration;

const SIMULATED_TOTAL_BYTES: u64 = 1_024 * 1_024 * 512; // 512 MiB
const SIMULATED_STEPS: u64 = 20;

/// Simulated flash for CLI output-mode testing (no real block I/O).
pub fn simulate_flash<F>(
    image: &str,
    device: &str,
    block_size: usize,
    silent: bool,
    verify: bool,
    mut progress: Option<F>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    emit(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Preparing)
            .with_message(format!("Opening image {image} (simulated)")),
    );

    thread::sleep(Duration::from_millis(80));

    for step in 1..=SIMULATED_STEPS {
        let bytes = SIMULATED_TOTAL_BYTES * step / SIMULATED_STEPS;
        let pct = if verify {
            (bytes as f64 / SIMULATED_TOTAL_BYTES as f64) * 90.0
        } else {
            (bytes as f64 / SIMULATED_TOTAL_BYTES as f64) * 100.0
        };
        emit(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Writing)
                .with_bytes(bytes, Some(SIMULATED_TOTAL_BYTES))
                .with_percentage(pct),
        );
        thread::sleep(Duration::from_millis(40));
    }

    if verify {
        emit(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Verifying)
                .with_percentage(90.0)
                .with_message("Verifying checksum (simulated)"),
        );

        for step in 1..=5 {
            let verified = SIMULATED_TOTAL_BYTES * step / 5;
            let pct = 90.0 + (verified as f64 / SIMULATED_TOTAL_BYTES as f64) * 10.0;
            emit(
                silent,
                &mut progress,
                OperationProgress::new(OperationPhase::Verifying)
                    .with_bytes(verified, Some(SIMULATED_TOTAL_BYTES))
                    .with_percentage(pct.min(99.9)),
            );
            thread::sleep(Duration::from_millis(40));
        }
    }

    emit(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Complete)
            .with_bytes(SIMULATED_TOTAL_BYTES, Some(SIMULATED_TOTAL_BYTES))
            .with_percentage(100.0)
            .with_message(format!(
                "Simulated flash of {image} to {device} (block_size={block_size}, verify={verify})"
            )),
    );

    Ok(())
}

/// Simulated clone for CLI output-mode testing (no real block I/O).
pub fn simulate_clone<F>(
    device: &str,
    file: &str,
    block_size: usize,
    silent: bool,
    mut progress: Option<F>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    emit(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Preparing)
            .with_message(format!("Opening {device} (simulated)")),
    );

    thread::sleep(Duration::from_millis(80));

    for step in 1..=SIMULATED_STEPS {
        let bytes = SIMULATED_TOTAL_BYTES * step / SIMULATED_STEPS;
        let pct = (bytes as f64 / SIMULATED_TOTAL_BYTES as f64) * 100.0;
        emit(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Writing)
                .with_bytes(bytes, Some(SIMULATED_TOTAL_BYTES))
                .with_percentage(pct),
        );
        thread::sleep(Duration::from_millis(40));
    }

    emit(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Complete)
            .with_bytes(SIMULATED_TOTAL_BYTES, Some(SIMULATED_TOTAL_BYTES))
            .with_percentage(100.0)
            .with_message(format!(
                "Simulated clone of {device} to {file} (block_size={block_size})"
            )),
    );

    Ok(())
}

fn emit<F>(silent: bool, progress: &mut Option<F>, event: OperationProgress)
where
    F: FnMut(OperationProgress),
{
    if silent {
        return;
    }
    if let Some(callback) = progress {
        callback(event);
    }
}
