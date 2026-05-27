use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::Result;

use super::builder::{Measurement, time_loop};
use super::config::MeasurementConfig;
use super::data::{RunResult, Sample};
use super::stats::Stats;

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
    warm_up(config.warmup_time(), &mut op);
    let iterations_per_sample = calibrate_iterations(config.min_sample_time(), &mut op);
    let mut samples = Vec::with_capacity(config.max_samples());
    let mut required_samples = config.min_samples();
    for sample_index in 0..config.max_samples() {
        samples.push(time_sample(
            linear_sample_iterations(iterations_per_sample, sample_index, config.min_samples()),
            &mut op,
            baseline.as_mut(),
        ));
        if samples.len() >= config.min_samples() {
            let stats = Stats::from_samples(&samples, config.confidence_z());
            required_samples = required_sample_count(
                stats.mean_ns,
                stats.stddev_ns,
                config.target_relative_error(),
                config.confidence_z(),
                config.min_samples(),
                config.max_samples(),
            );
            if samples.len() >= required_samples {
                break;
            }
        }
    }
    let stats = Stats::from_samples(&samples, config.confidence_z());
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
    mut measurement: Measurement,
) -> Result<RunResult> {
    measurement.warm_up(config.warmup_time());
    let iterations_per_sample =
        calibrate_measurement_iterations(config.min_sample_time(), &mut measurement);
    let (samples, required_samples) = collect_adaptive_samples(
        config,
        &mut measurement,
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
    measurement: &mut Measurement,
    iterations_per_sample: u64,
    mut on_sample: impl FnMut(usize, &Sample),
) -> (Vec<Sample>, usize) {
    let mut samples = Vec::with_capacity(config.max_samples());
    let mut required_samples = config.min_samples();
    for sample_index in 0..config.max_samples() {
        let sample = measurement.time_sample(linear_sample_iterations(
            iterations_per_sample,
            sample_index,
            required_samples,
        ));
        on_sample(sample_index, &sample);
        samples.push(sample);
        if samples.len() >= config.min_samples() {
            let stats = Stats::from_samples(&samples, config.confidence_z());
            required_samples = required_sample_count(
                stats.mean_ns,
                stats.stddev_ns,
                config.target_relative_error(),
                config.confidence_z(),
                config.min_samples(),
                config.max_samples(),
            );
            if samples.len() >= required_samples {
                break;
            }
        }
    }
    (samples, required_samples)
}

pub fn collect_fixed_samples(
    measurement: &mut Measurement,
    iterations_per_sample: u64,
    samples: usize,
    mut on_sample: impl FnMut(usize, &Sample),
) -> Vec<Sample> {
    (0..samples)
        .map(|sample_index| {
            let sample = measurement.time_sample(linear_sample_iterations(
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

pub fn calibrate_measurement_iterations(
    min_sample_time: Duration,
    measurement: &mut Measurement,
) -> u64 {
    let mut iterations = 1_u64;
    loop {
        let elapsed = measurement.time_sample(iterations).elapsed;
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
