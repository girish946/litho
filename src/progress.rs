use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Preparing,
    Decompressing,
    Writing,
    Verifying,
    Complete,
    Failed,
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
    use super::*;

    #[test]
    fn percentage_from_bytes() {
        let p = OperationProgress::new(OperationPhase::Writing).with_bytes(50, Some(200));
        assert!((p.percentage.unwrap() - 25.0).abs() < f64::EPSILON);
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
