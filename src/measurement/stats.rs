use super::data::Sample;

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
