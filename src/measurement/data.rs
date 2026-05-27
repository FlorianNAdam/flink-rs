use std::time::Duration;

use super::stats::Stats;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub iterations: u64,
    pub elapsed: Duration,
    pub baseline_elapsed: Option<Duration>,
}

impl Sample {
    pub fn total_ns_per_iter(&self) -> f64 {
        nanos_per_iter(self.elapsed, self.iterations)
    }

    pub fn baseline_ns_per_iter(&self) -> Option<f64> {
        self.baseline_elapsed
            .map(|elapsed| nanos_per_iter(elapsed, self.iterations))
    }

    pub fn ns_per_iter(&self) -> f64 {
        (self.total_ns_per_iter() - self.baseline_ns_per_iter().unwrap_or(0.0)).max(0.0)
    }
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub iterations_per_sample: u64,
    pub required_samples: usize,
    pub samples: Vec<Sample>,
    pub stats: Stats,
    pub confidence_interval_is_heuristic: bool,
}

pub(super) fn nanos_per_iter(elapsed: Duration, iterations: u64) -> f64 {
    elapsed.as_nanos() as f64 / iterations.max(1) as f64
}
