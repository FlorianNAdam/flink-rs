use crate::Result;
use anyhow::ensure;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementConfig {
    warmup_time: Duration,
    min_sample_time: Duration,
    target_relative_error: f64,
    min_samples: usize,
    max_samples: usize,
    confidence_z: f64,
}

impl From<&MeasurementConfig> for MeasurementConfig {
    fn from(config: &MeasurementConfig) -> Self {
        *config
    }
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            warmup_time: Duration::from_secs(1),
            min_sample_time: Duration::from_millis(100),
            target_relative_error: 0.02,
            min_samples: 20,
            max_samples: 100,
            confidence_z: 1.96,
        }
    }
}

impl MeasurementConfig {
    pub fn new(
        warmup_time: Duration,
        min_sample_time: Duration,
        target_relative_error: f64,
        min_samples: usize,
        max_samples: usize,
        confidence_z: f64,
    ) -> Result<Self> {
        ensure!(
            !warmup_time.is_zero(),
            "warmup_time must be greater than zero"
        );
        ensure!(
            !min_sample_time.is_zero(),
            "min_sample_time must be greater than zero"
        );
        ensure!(
            target_relative_error.is_finite() && target_relative_error > 0.0,
            "target_relative_error must be finite and greater than zero",
        );
        ensure!(min_samples > 1, "min_samples must be greater than one");
        ensure!(
            max_samples >= min_samples,
            "max_samples must be >= min_samples",
        );
        ensure!(
            confidence_z.is_finite() && confidence_z > 0.0,
            "confidence_z must be finite and greater than zero",
        );
        Ok(Self {
            warmup_time,
            min_sample_time,
            target_relative_error,
            min_samples,
            max_samples,
            confidence_z,
        })
    }

    pub fn warmup_time(&self) -> Duration {
        self.warmup_time
    }

    pub fn min_sample_time(&self) -> Duration {
        self.min_sample_time
    }

    pub fn target_relative_error(&self) -> f64 {
        self.target_relative_error
    }

    pub fn min_samples(&self) -> usize {
        self.min_samples
    }

    pub fn max_samples(&self) -> usize {
        self.max_samples
    }

    pub fn confidence_z(&self) -> f64 {
        self.confidence_z
    }
}
