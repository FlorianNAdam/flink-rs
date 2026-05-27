use std::hint::black_box;
use std::time::{Duration, Instant};

use super::data::Sample;

pub trait Workload {
    fn warm_up(&mut self, warmup_time: Duration);
    fn time_sample(&mut self, iterations: u64) -> Sample;
}

pub struct WorkloadFn {
    time_op: Box<dyn FnMut(u64) -> Duration>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

pub struct MeasurementBuilder {}

impl MeasurementBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn measure<T>(self, mut op: impl FnMut() -> T + 'static) -> WorkloadBuilder
    where
        T: 'static,
    {
        WorkloadBuilder {
            time_op: Box::new(move |iterations| time_loop(iterations, &mut op)),
            time_baseline: None,
        }
    }
}

pub struct WorkloadBuilder {
    time_op: Box<dyn FnMut(u64) -> Duration>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

impl WorkloadBuilder {
    pub fn baseline<B>(mut self, mut baseline: impl FnMut() -> B + 'static) -> Self
    where
        B: 'static,
    {
        self.time_baseline = Some(Box::new(move |iterations| {
            time_loop(iterations, &mut baseline)
        }));
        self
    }

    pub fn build(self) -> WorkloadFn {
        WorkloadFn {
            time_op: self.time_op,
            time_baseline: self.time_baseline,
        }
    }
}

impl Workload for WorkloadFn {
    fn warm_up(&mut self, warmup_time: Duration) {
        let start = Instant::now();
        while start.elapsed() < warmup_time {
            (self.time_op)(1);
        }
    }

    fn time_sample(&mut self, iterations: u64) -> Sample {
        let baseline_elapsed = self
            .time_baseline
            .as_mut()
            .map(|baseline| baseline(iterations));

        Sample {
            iterations,
            elapsed: (self.time_op)(iterations),
            baseline_elapsed,
        }
    }
}

pub(super) fn time_loop<T>(iterations: u64, op: &mut impl FnMut() -> T) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(op());
    }
    start.elapsed()
}
