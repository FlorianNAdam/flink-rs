use std::hint::black_box;
use std::time::{Duration, Instant};

use anyhow::anyhow;

use crate::Result;

use super::data::Sample;

pub struct Measurement {
    time_op: Box<dyn FnMut(u64) -> Duration>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

pub struct MeasurementBuilder {
    time_op: Option<Box<dyn FnMut(u64) -> Duration>>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

impl MeasurementBuilder {
    pub fn new() -> Self {
        Self {
            time_op: None,
            time_baseline: None,
        }
    }

    pub fn measure<T>(mut self, mut op: impl FnMut() -> T + 'static) -> Self
    where
        T: 'static,
    {
        self.time_op = Some(Box::new(move |iterations| time_loop(iterations, &mut op)));
        self
    }

    pub fn baseline<B>(mut self, mut baseline: impl FnMut() -> B + 'static) -> Self
    where
        B: 'static,
    {
        self.time_baseline = Some(Box::new(move |iterations| {
            time_loop(iterations, &mut baseline)
        }));
        self
    }

    pub fn build(self) -> Result<Measurement> {
        Ok(Measurement {
            time_op: self.time_op.ok_or_else(|| anyhow!("missing measurement"))?,
            time_baseline: self.time_baseline,
        })
    }
}

impl Measurement {
    pub(crate) fn warm_up(&mut self, warmup_time: Duration) {
        let start = Instant::now();
        while start.elapsed() < warmup_time {
            (self.time_op)(1);
        }
    }

    pub(crate) fn time_sample(&mut self, iterations: u64) -> Sample {
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
