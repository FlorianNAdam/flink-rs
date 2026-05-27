use crate::Result;
use crate::definition::{Benchmark, Case};
use crate::measurement::{
    Measured, MeasurementConfig, RunResult, Sample, calibrate_measured_iterations,
    collect_adaptive_samples, collect_fixed_samples, run_result_from_samples,
};
use crate::runner::RunnerHook;

pub trait Executor {
    fn run_set(
        &self,
        benchmark: &Benchmark,
        cases: &[Case],
        measurement: &MeasurementConfig,
        hooks: &mut dyn RunnerHook,
    ) -> Result<Vec<RunResult>>;
}

impl Executor for Box<dyn Executor> {
    fn run_set(
        &self,
        benchmark: &Benchmark,
        cases: &[Case],
        measurement: &MeasurementConfig,
        hooks: &mut dyn RunnerHook,
    ) -> Result<Vec<RunResult>> {
        self.as_ref().run_set(benchmark, cases, measurement, hooks)
    }
}

pub struct AdaptiveExecutor;

impl Executor for AdaptiveExecutor {
    fn run_set(
        &self,
        benchmark: &Benchmark,
        cases: &[Case],
        measurement: &MeasurementConfig,
        hooks: &mut dyn RunnerHook,
    ) -> Result<Vec<RunResult>> {
        cases
            .iter()
            .map(|case| {
                hooks.on_case_start(case);
                let mut measured = benchmark.measure_case(case)?;
                hooks.on_case_warmup_start(case);
                measured.warm_up(measurement.warmup_time());
                hooks.on_case_warmup_finish(case);
                hooks.on_case_calibration_start(case);
                let iterations_per_sample =
                    calibrate_measured_iterations(measurement.min_sample_time(), &mut measured);
                hooks.on_case_calibration_finish(case, iterations_per_sample);
                hooks.on_case_collection_start(
                    case,
                    iterations_per_sample,
                    measurement.max_samples(),
                );
                let (samples, required_samples) = collect_adaptive_samples(
                    measurement,
                    &mut measured,
                    iterations_per_sample,
                    |sample_index, sample| {
                        hooks.on_case_collection_sample(case, sample_index, sample);
                    },
                );
                let result = run_result_from_samples(
                    measurement,
                    iterations_per_sample,
                    required_samples,
                    samples,
                );
                hooks.on_case_collection_finish(case, &result);
                hooks.on_case_finish(case, &result);
                Ok(result)
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedExecutor {
    samples: usize,
}

impl FixedExecutor {
    pub fn new(samples: usize) -> Self {
        Self { samples }
    }
}

impl Default for FixedExecutor {
    fn default() -> Self {
        Self { samples: 30 }
    }
}

impl Executor for FixedExecutor {
    fn run_set(
        &self,
        benchmark: &Benchmark,
        cases: &[Case],
        measurement: &MeasurementConfig,
        hooks: &mut dyn RunnerHook,
    ) -> Result<Vec<RunResult>> {
        let mut measured = cases
            .iter()
            .map(|case| benchmark.measure_case(case))
            .collect::<Result<Vec<_>>>()?;

        hooks.on_set_calibration_start(cases);
        let iterations_per_sample = measured
            .iter_mut()
            .map(|measured| {
                hooks.on_set_warmup_start(&cases);
                measured.warm_up(measurement.warmup_time());
                hooks.on_set_warmup_finish(0);

                hooks.on_set_calibration_start(&cases);
                let ips = calibrate_measured_iterations(measurement.min_sample_time(), measured);
                hooks.on_set_calibration_finish(ips);

                ips
            })
            .max()
            .unwrap_or(1)
            .next_power_of_two();
        hooks.on_set_calibration_finish(iterations_per_sample);

        cases
            .iter()
            .zip(measured.iter_mut())
            .map(|(case, measured)| {
                hooks.on_case_start(case);

                hooks.on_case_warmup_start(case);
                measured.warm_up(measurement.warmup_time());
                hooks.on_case_warmup_finish(case);

                hooks.on_case_collection_start(case, iterations_per_sample, self.samples);
                let result = run_fixed_measurement(
                    measurement,
                    measured,
                    iterations_per_sample,
                    self.samples,
                    |sample_index, sample| {
                        hooks.on_case_collection_sample(case, sample_index, sample);
                    },
                );
                hooks.on_case_collection_finish(case, &result);

                hooks.on_case_finish(case, &result);
                Ok(result)
            })
            .collect()
    }
}

fn run_fixed_measurement(
    config: &MeasurementConfig,
    measured: &mut dyn Measured,
    iterations_per_sample: u64,
    samples: usize,
    on_sample: impl FnMut(usize, &Sample),
) -> RunResult {
    let samples = collect_fixed_samples(measured, iterations_per_sample, samples, on_sample);
    run_result_from_samples(config, iterations_per_sample, samples.len(), samples)
}
