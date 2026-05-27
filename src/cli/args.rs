use std::time::Duration;

use clap;

use crate::runner::RunnerConfig;

#[derive(clap::Args, Debug, Clone)]
pub struct RunnerArgs {
    /// Warmup time (e.g. "500ms", "2s")
    #[arg(long, value_parser = humantime::parse_duration, default_value = "1s")]
    pub warmup_time: Duration,

    /// Minimum sample collection time (e.g. "50ms", "200ms")
    #[arg(long, value_parser = humantime::parse_duration, default_value = "100ms")]
    pub min_sample_time: Duration,

    /// Target relative error for confidence interval
    #[arg(long, default_value_t = 0.02)]
    pub target_relative_error: f64,

    /// Minimum number of samples per case
    #[arg(long, default_value_t = 20)]
    pub min_samples: u32,

    /// Maximum number of samples per case
    #[arg(long, default_value_t = 100)]
    pub max_samples: u32,

    /// Confidence z-score (1.96 ≈ 95%)
    #[arg(long, default_value_t = 1.96)]
    pub confidence_z: f64,
}

impl RunnerArgs {
    pub fn to_runner_config(&self) -> crate::Result<RunnerConfig> {
        Ok(RunnerConfig::new(crate::MeasurementConfig::new(
            self.warmup_time,
            self.min_sample_time,
            self.target_relative_error,
            self.min_samples as usize,
            self.max_samples as usize,
            self.confidence_z,
        )?))
    }
}
