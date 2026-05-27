use crate::Result;
use anyhow::ensure;
use std::hint::black_box;
use std::time::{Duration, Instant};

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub mean_ns: f64,
    pub lower_bound_ns: f64,
    pub upper_bound_ns: f64,
    pub median_ns: f64,
    pub stddev_ns: f64,
    pub p95_ns: f64,
    pub ci_half_width_ns: f64,
    pub relative_ci_half_width: f64,
}

impl Stats {
    pub fn from_samples(samples: &[Sample], confidence_z: f64) -> Self {
        assert!(!samples.is_empty(), "samples must not be empty");
        let mut values: Vec<f64> = samples.iter().map(Sample::ns_per_iter).collect();
        values.sort_by(|a, b| a.total_cmp(b));
        let arithmetic_mean_ns = values.iter().sum::<f64>() / values.len() as f64;
        let median_ns = percentile_sorted(&values, 0.5);
        let p95_ns = percentile_sorted(&values, 0.95);
        let stddev_ns = sample_variance(&values, arithmetic_mean_ns).sqrt();
        let estimate = bootstrap_regression_estimate(samples, confidence_z).unwrap_or_else(|| {
            mean_estimate(arithmetic_mean_ns, stddev_ns, values.len(), confidence_z)
        });
        let mean_ns = estimate.point;
        let ci_half_width_ns = (estimate.point - estimate.lower)
            .max(estimate.upper - estimate.point)
            .max(0.0);
        let relative_ci_half_width = if mean_ns > 0.0 {
            ci_half_width_ns / mean_ns
        } else {
            0.0
        };
        Self {
            mean_ns,
            lower_bound_ns: estimate.lower,
            upper_bound_ns: estimate.upper,
            median_ns,
            stddev_ns,
            p95_ns,
            ci_half_width_ns,
            relative_ci_half_width,
        }
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

pub trait Measured {
    fn warm_up(&mut self, warmup_time: Duration);
    fn time_sample(&mut self, iterations: u64) -> Sample;
}

pub struct MeasuredFn {
    time_op: Box<dyn FnMut(u64) -> Duration>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

pub struct MeasurementBuilder {}

impl MeasurementBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn measure<T>(self, mut op: impl FnMut() -> T + 'static) -> MeasureBuilder
    where
        T: 'static,
    {
        MeasureBuilder {
            time_op: Box::new(move |iterations| time_loop(iterations, &mut op)),
            time_baseline: None,
        }
    }
}

pub struct MeasureBuilder {
    time_op: Box<dyn FnMut(u64) -> Duration>,
    time_baseline: Option<Box<dyn FnMut(u64) -> Duration>>,
}

impl MeasureBuilder {
    pub fn baseline<B>(mut self, mut baseline: impl FnMut() -> B + 'static) -> Self
    where
        B: 'static,
    {
        self.time_baseline = Some(Box::new(move |iterations| {
            time_loop(iterations, &mut baseline)
        }));
        self
    }

    pub fn build(self) -> MeasuredFn {
        MeasuredFn {
            time_op: self.time_op,
            time_baseline: self.time_baseline,
        }
    }
}

pub fn measure<T>(config: &MeasurementConfig, op: impl FnMut() -> T) -> Result<RunResult> {
    measure_with_optional_baseline(config, op, Option::<fn()>::None)
}

pub fn measure_with_baseline<T, B>(
    config: &MeasurementConfig,
    op: impl FnMut() -> T,
    baseline: impl FnMut() -> B,
) -> Result<RunResult> {
    measure_with_optional_baseline(config, op, Some(baseline))
}

fn measure_with_optional_baseline<T, B>(
    config: &MeasurementConfig,
    mut op: impl FnMut() -> T,
    mut baseline: Option<impl FnMut() -> B>,
) -> Result<RunResult> {
    warm_up(config.warmup_time, &mut op);
    let iterations_per_sample = calibrate_iterations(config.min_sample_time, &mut op);
    let mut samples = Vec::with_capacity(config.max_samples);
    let mut required_samples = config.min_samples;
    for sample_index in 0..config.max_samples {
        samples.push(time_sample(
            linear_sample_iterations(iterations_per_sample, sample_index, config.min_samples),
            &mut op,
            baseline.as_mut(),
        ));
        if samples.len() >= config.min_samples {
            let stats = Stats::from_samples(&samples, config.confidence_z);
            required_samples = required_sample_count(
                stats.mean_ns,
                stats.stddev_ns,
                config.target_relative_error,
                config.confidence_z,
                config.min_samples,
                config.max_samples,
            );
            if samples.len() >= required_samples {
                break;
            }
        }
    }
    let stats = Stats::from_samples(&samples, config.confidence_z);
    Ok(RunResult {
        iterations_per_sample,
        required_samples,
        samples,
        stats,
        confidence_interval_is_heuristic: true,
    })
}

pub fn run_adaptive_measurement(
    config: &MeasurementConfig,
    mut measured: impl Measured,
) -> Result<RunResult> {
    measured.warm_up(config.warmup_time);
    let iterations_per_sample =
        calibrate_measured_iterations(config.min_sample_time, &mut measured);
    let (samples, required_samples) = collect_adaptive_samples(
        config,
        &mut measured,
        iterations_per_sample,
        |_sample_index, _sample| {},
    );
    Ok(run_result_from_samples(
        config,
        iterations_per_sample,
        required_samples,
        samples,
    ))
}

pub fn collect_adaptive_samples(
    config: &MeasurementConfig,
    measured: &mut dyn Measured,
    iterations_per_sample: u64,
    mut on_sample: impl FnMut(usize, &Sample),
) -> (Vec<Sample>, usize) {
    let mut samples = Vec::with_capacity(config.max_samples);
    let mut required_samples = config.min_samples;
    for sample_index in 0..config.max_samples {
        let sample = measured.time_sample(linear_sample_iterations(
            iterations_per_sample,
            sample_index,
            required_samples,
        ));
        on_sample(sample_index, &sample);
        samples.push(sample);
        if samples.len() >= config.min_samples {
            let stats = Stats::from_samples(&samples, config.confidence_z);
            required_samples = required_sample_count(
                stats.mean_ns,
                stats.stddev_ns,
                config.target_relative_error,
                config.confidence_z,
                config.min_samples,
                config.max_samples,
            );
            if samples.len() >= required_samples {
                break;
            }
        }
    }
    (samples, required_samples)
}

pub fn collect_fixed_samples(
    measured: &mut dyn Measured,
    iterations_per_sample: u64,
    samples: usize,
    mut on_sample: impl FnMut(usize, &Sample),
) -> Vec<Sample> {
    (0..samples)
        .map(|sample_index| {
            let sample = measured.time_sample(linear_sample_iterations(
                iterations_per_sample,
                sample_index,
                samples,
            ));
            on_sample(sample_index, &sample);
            sample
        })
        .collect()
}

pub fn run_result_from_samples(
    config: &MeasurementConfig,
    iterations_per_sample: u64,
    required_samples: usize,
    samples: Vec<Sample>,
) -> RunResult {
    let stats = Stats::from_samples(&samples, config.confidence_z());
    RunResult {
        iterations_per_sample,
        required_samples,
        samples,
        stats,
        confidence_interval_is_heuristic: true,
    }
}

pub fn calibrate_measured_iterations(
    min_sample_time: Duration,
    measured: &mut dyn Measured,
) -> u64 {
    let mut iterations = 1_u64;
    loop {
        let elapsed = measured.time_sample(iterations).elapsed;
        if elapsed >= min_sample_time {
            return iterations;
        }
        let elapsed_ns = elapsed.as_nanos().max(1);
        let target_ns = min_sample_time.as_nanos().max(1);
        iterations = iterations.saturating_mul((target_ns.div_ceil(elapsed_ns) as u64).max(2));
        if iterations == u64::MAX {
            return iterations;
        }
    }
}

pub fn calibrate_iterations<T>(min_sample_time: Duration, op: &mut impl FnMut() -> T) -> u64 {
    let mut iterations = 1_u64;
    loop {
        let elapsed = time_loop(iterations, op);
        if elapsed >= min_sample_time {
            return iterations;
        }
        let elapsed_ns = elapsed.as_nanos().max(1);
        let target_ns = min_sample_time.as_nanos().max(1);
        iterations = iterations.saturating_mul((target_ns.div_ceil(elapsed_ns) as u64).max(2));
        if iterations == u64::MAX {
            return iterations;
        }
    }
}

pub fn required_sample_count(
    mean_ns: f64,
    stddev_ns: f64,
    relative_error: f64,
    confidence_z: f64,
    min_samples: usize,
    max_samples: usize,
) -> usize {
    if mean_ns <= 0.0 || stddev_ns <= 0.0 || relative_error <= 0.0 || confidence_z <= 0.0 {
        return min_samples;
    }
    let cv = stddev_ns / mean_ns;
    ((confidence_z * cv / relative_error).powi(2).ceil() as usize)
        .max(min_samples)
        .min(max_samples)
}

fn warm_up<T>(warmup_time: Duration, op: &mut impl FnMut() -> T) {
    let start = Instant::now();
    while start.elapsed() < warmup_time {
        black_box(op());
    }
}

fn time_sample<T, B>(
    iterations: u64,
    op: &mut impl FnMut() -> T,
    baseline: Option<&mut impl FnMut() -> B>,
) -> Sample {
    let baseline_elapsed = baseline.map(|baseline| time_loop(iterations, baseline));
    let elapsed = time_loop(iterations, op);
    Sample {
        iterations,
        elapsed,
        baseline_elapsed,
    }
}

impl Measured for MeasuredFn {
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

fn time_loop<T>(iterations: u64, op: &mut impl FnMut() -> T) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(op());
    }
    start.elapsed()
}

pub fn linear_sample_iterations(
    iterations_per_sample: u64,
    sample_index: usize,
    samples: usize,
) -> u64 {
    let samples = samples.max(1) as u128;
    let numerator = iterations_per_sample as u128 * 2 * (sample_index as u128 + 1);
    let denominator = samples + 1;
    (numerator / denominator).max(1).min(u64::MAX as u128) as u64
}

#[derive(Debug, Clone, Copy)]
struct Estimate {
    point: f64,
    lower: f64,
    upper: f64,
}

fn mean_estimate(mean_ns: f64, stddev_ns: f64, samples: usize, confidence_z: f64) -> Estimate {
    let ci_half_width_ns = confidence_z * stddev_ns / (samples as f64).sqrt();
    Estimate {
        point: mean_ns,
        lower: (mean_ns - ci_half_width_ns).max(0.0),
        upper: mean_ns + ci_half_width_ns,
    }
}

fn bootstrap_regression_estimate(samples: &[Sample], confidence_z: f64) -> Option<Estimate> {
    let point = regression_slope(samples)?;
    let confidence = confidence_from_z(confidence_z);
    let lower_percentile = (1.0 - confidence) / 2.0;
    let upper_percentile = 1.0 - lower_percentile;
    let mut rng = BootstrapRng::new(bootstrap_seed(samples));
    let mut resampled = Vec::with_capacity(samples.len());
    let mut slopes = Vec::with_capacity(BOOTSTRAP_RESAMPLES);

    for _ in 0..BOOTSTRAP_RESAMPLES {
        resampled.clear();
        for _ in 0..samples.len() {
            let index = rng.next_index(samples.len());
            resampled.push(samples[index]);
        }
        if let Some(slope) = regression_slope(&resampled) {
            slopes.push(slope);
        }
    }

    if slopes.is_empty() {
        return None;
    }

    slopes.sort_by(|a, b| a.total_cmp(b));
    Some(Estimate {
        point,
        lower: percentile_sorted(&slopes, lower_percentile),
        upper: percentile_sorted(&slopes, upper_percentile),
    })
}

fn regression_slope(samples: &[Sample]) -> Option<f64> {
    if samples.len() <= 1 {
        return None;
    }

    let n = samples.len() as f64;
    let mean_x = samples
        .iter()
        .map(|sample| sample.iterations as f64)
        .sum::<f64>()
        / n;
    let mean_y = samples.iter().map(sample_elapsed_ns).sum::<f64>() / n;

    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for sample in samples {
        let dx = sample.iterations as f64 - mean_x;
        let dy = sample_elapsed_ns(sample) - mean_y;
        sxx += dx * dx;
        sxy += dx * dy;
    }
    if sxx <= 0.0 {
        return None;
    }

    let slope = sxy / sxx;
    if !slope.is_finite() || slope < 0.0 {
        return None;
    }

    Some(slope)
}

fn confidence_from_z(z: f64) -> f64 {
    (2.0 * normal_cdf(z.abs()) - 1.0).clamp(0.0, 0.999_999)
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / 2.0_f64.sqrt()))
}

fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

fn bootstrap_seed(samples: &[Sample]) -> u64 {
    let mut seed = 0x517c_c1b7_2722_0a95_u64;
    for sample in samples {
        seed = seed.rotate_left(5) ^ sample.iterations;
        seed = seed.rotate_left(5) ^ sample.elapsed.as_nanos() as u64;
        seed = seed.rotate_left(5)
            ^ sample
                .baseline_elapsed
                .map(|elapsed| elapsed.as_nanos() as u64)
                .unwrap_or(0);
    }
    seed
}

struct BootstrapRng {
    state: u64,
}

impl BootstrapRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_index(&mut self, len: usize) -> usize {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.state >> 32) as usize) % len
    }
}

const BOOTSTRAP_RESAMPLES: usize = 10_000;

fn sample_elapsed_ns(sample: &Sample) -> f64 {
    let elapsed_ns = sample.elapsed.as_nanos() as f64;
    let baseline_ns = sample
        .baseline_elapsed
        .map(|elapsed| elapsed.as_nanos() as f64)
        .unwrap_or(0.0);
    (elapsed_ns - baseline_ns).max(0.0)
}

fn nanos_per_iter(elapsed: Duration, iterations: u64) -> f64 {
    elapsed.as_nanos() as f64 / iterations.max(1) as f64
}

fn sample_variance(values: &[f64], mean: f64) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    values
        .iter()
        .map(|value| {
            let diff = value - mean;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64
}

fn percentile_sorted(values: &[f64], percentile: f64) -> f64 {
    debug_assert!(!values.is_empty());
    let rank = percentile.clamp(0.0, 1.0) * (values.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        values[low]
    } else {
        let weight = rank - low as f64;
        values[low] * (1.0 - weight) + values[high] * weight
    }
}
