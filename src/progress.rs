use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Preparing,
    Decompressing,
    Writing,
    Verifying,
    Complete,
    Failed,
    Cancelled,
}

/// Returned when an in-flight flash/clone stops because the caller set the cancel flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationCancelled;

impl std::fmt::Display for OperationCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation cancelled")
    }
}

impl std::error::Error for OperationCancelled {}

/// Best-effort cooperative cancel check for block I/O loops.
pub fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), OperationCancelled> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        Err(OperationCancelled)
    } else {
        Ok(())
    }
}

pub fn is_operation_cancelled(err: &anyhow::Error) -> bool {
    err.downcast_ref::<OperationCancelled>().is_some()
}

/// Line written to litho stdin by GUI hosts (e.g. Lithographer) to request cancel.
/// Works across privilege boundaries where signals from the parent cannot reach pkexec.
pub const STDIN_CANCEL_LINE: &str = "cancel";

pub fn is_stdin_cancel_line(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case(STDIN_CANCEL_LINE)
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationProgress {
    pub phase: OperationPhase,
    pub bytes_processed: u64,
    pub bytes_total: Option<u64>,
    pub percentage: Option<f64>,
    pub message: Option<String>,
}

impl OperationProgress {
    pub fn new(phase: OperationPhase) -> Self {
        Self {
            phase,
            bytes_processed: 0,
            bytes_total: None,
            percentage: None,
            message: None,
        }
    }

    pub fn with_bytes(mut self, processed: u64, total: Option<u64>) -> Self {
        self.bytes_processed = processed;
        self.bytes_total = total;
        self.percentage = total.and_then(|t| {
            if t == 0 {
                None
            } else {
                Some((processed as f64 / t as f64) * 100.0)
            }
        });
        self
    }

    pub fn with_percentage(mut self, percentage: f64) -> Self {
        self.percentage = Some(percentage.clamp(0.0, 100.0));
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

pub(crate) fn emit_progress<F>(silent: bool, progress: &mut Option<F>, event: OperationProgress)
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

#[cfg(test)]
mod tests {
    use super::{is_stdin_cancel_line, STDIN_CANCEL_LINE, *};

    #[test]
    fn percentage_from_bytes() {
        let p = OperationProgress::new(OperationPhase::Writing).with_bytes(50, Some(200));
        assert!((p.percentage.unwrap() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn check_cancel_passes_when_flag_clear() {
        let flag = AtomicBool::new(false);
        assert!(check_cancel(Some(&flag)).is_ok());
        assert!(check_cancel(None).is_ok());
    }

    #[test]
    fn check_cancel_errors_when_flag_set() {
        let flag = AtomicBool::new(true);
        assert_eq!(check_cancel(Some(&flag)), Err(OperationCancelled));
    }

    #[test]
    fn stdin_cancel_line_is_stable() {
        assert!(is_stdin_cancel_line(STDIN_CANCEL_LINE));
        assert!(is_stdin_cancel_line("Cancel"));
    }

    #[test]
    fn cancelled_phase_serializes() {
        let json = serde_json::to_string(&OperationPhase::Cancelled).unwrap();
        assert_eq!(json, "\"cancelled\"");
    }

    #[test]
    fn clone_style_percentage_at_half() {
        let total_sectors = 1_000_000u64;
        let bytes_done = total_sectors * 256; // half of 512-byte sectors
        let total_bytes = total_sectors * 512;
        let p = OperationProgress::new(OperationPhase::Writing)
            .with_bytes(bytes_done, Some(total_bytes));
        assert!((p.percentage.unwrap() - 50.0).abs() < f64::EPSILON);
    }
}
