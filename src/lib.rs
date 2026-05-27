//! Small evaluation helpers for timing functions across variants and inputs.
//!
//! The workload closure is the benchmarked operation. If an operation mutates
//! state, include the reset, rebuild, or fresh-state construction inside that
//! closure so every iteration measures the same semantics. Use
//! [`measure_with_baseline`] when a cheap loop/setup baseline should be sampled
//! and subtracted from each timing sample. Build immutable state before creating
//! the workload closure when it should be excluded from measurement.

#[cfg(feature = "cli")]
pub mod cli;
mod definition;
pub mod io;
mod measurement;
mod runner;
mod util;

pub use anyhow::Result;
pub use definition::{
    Benchmark, BenchmarkPath, BenchmarkSuite, BenchmarkVariant, Case, CaseId, CaseInput,
    CaseRunner, InputDimension, InputOverrideSpec, InputOverrides, InputRange, IntoBenchmarkPath,
};
pub use measurement::{
    MeasurementBuilder, MeasurementConfig, RunResult, Sample, Stats, Workload, WorkloadFn,
    calibrate_iterations, measure, measure_with_baseline, required_sample_count,
    run_adaptive_measurement,
};
pub use runner::{
    AdaptiveExecutor, BenchmarkResult, BenchmarkResults, Executor, FilterByPaths,
    FilterByPathsExact, FilterInverse, FilterPredicateBuilder, FixedExecutor, Predicate, Runner,
    RunnerConfig, RunnerHook,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stats_reports_basic_values() {
        let samples = [
            sample(10.0),
            sample(20.0),
            sample(30.0),
            sample(40.0),
            sample(50.0),
        ];
        let stats = Stats::from_samples(&samples, 1.96);
        assert_eq!(stats.mean_ns, 30.0);
        assert!(stats.lower_bound_ns <= stats.mean_ns);
        assert!(stats.upper_bound_ns >= stats.mean_ns);
        assert_eq!(stats.median_ns, 30.0);
        assert_eq!(stats.p95_ns, 48.0);
        assert!(stats.relative_ci_half_width > 0.0);
    }

    #[test]
    fn stats_uses_regression_slope_for_varied_iterations() {
        let samples = [
            timed_sample(100, 1_500),
            timed_sample(200, 2_500),
            timed_sample(300, 3_500),
            timed_sample(400, 4_500),
        ];
        let stats = Stats::from_samples(&samples, 1.96);

        assert_eq!(stats.mean_ns, 10.0);
        assert_eq!(stats.lower_bound_ns, 10.0);
        assert_eq!(stats.upper_bound_ns, 10.0);
        assert_eq!(stats.ci_half_width_ns, 0.0);
    }

    #[test]
    fn stats_subtracts_baseline_before_regression() {
        let samples = [
            baseline_sample(100, 1_800, 300),
            baseline_sample(200, 3_100, 600),
            baseline_sample(300, 4_400, 900),
            baseline_sample(400, 5_700, 1_200),
        ];
        let stats = Stats::from_samples(&samples, 1.96);

        assert_eq!(stats.mean_ns, 10.0);
        assert_eq!(stats.lower_bound_ns, 10.0);
        assert_eq!(stats.upper_bound_ns, 10.0);
        assert_eq!(stats.ci_half_width_ns, 0.0);
    }

    #[test]
    fn benchmark_definition_expands_defaults() {
        let definition = Benchmark::new("join")
            .input("left", InputRange::values([10, 20]))
            .input("right", InputRange::values([100, 200]))
            .variant("hash", noop_case)
            .variant("nested", noop_case);
        let cases = definition.expand_cases(&InputOverrides::new()).unwrap();
        assert_eq!(cases.len(), 8);
        assert_eq!(cases[0].input("left"), Some(10));
        assert_eq!(cases[0].input("right"), Some(100));
    }

    #[test]
    fn runner_builder_applies_path_overrides() {
        let runner = Runner::new(RunnerConfig::new(tiny_config()));

        let outputs = runner
            .suite("examples", |suite| {
                suite.suite("nested", |suite| {
                    suite.benchmark("sum_small", |benchmark| {
                        benchmark
                            .input("rows", InputRange::values([1]))
                            .variant("sum", noop_case)
                    })
                })
            })
            .override_input(
                "examples.nested.sum_small",
                "rows",
                InputRange::values([7, 9]),
            )
            .run()
            .unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].id.input.as_str(), "rows=7");
        assert_eq!(outputs[1].id.input.as_str(), "rows=9");
    }

    #[test]
    fn filter_skips_benchmarks() {
        let runner = Runner::new(RunnerConfig::new(tiny_config()));

        let outputs = runner
            .suite("examples", |suite| {
                suite
                    .benchmark("keep", |b| b.variant("v", noop_case))
                    .benchmark("skip", |b| b.variant("v", noop_case))
            })
            .filter(|f| {
                f.by_paths_exact(vec![BenchmarkPath::from_parts(vec![
                    "examples".into(),
                    "keep".into(),
                ])])
            })
            .run()
            .unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].id.benchmark, "keep");
    }

    #[test]
    fn filter_inverse_skips_specified_benchmark() {
        let runner = Runner::new(RunnerConfig::new(tiny_config()));

        let outputs = runner
            .suite("s", |suite| {
                suite
                    .benchmark("done", |b| b.variant("v", noop_case))
                    .benchmark("new", |b| b.variant("v", noop_case))
            })
            .filter(|f| {
                f.inverse(|f| {
                    f.by_paths_exact(vec![BenchmarkPath::from_parts(vec![
                        "s".into(),
                        "done".into(),
                    ])])
                })
            })
            .run()
            .unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].id.benchmark, "new");
    }

    #[test]
    fn runner_builder_calls_hooks() {
        use std::cell::RefCell;
        use std::rc::Rc;

        struct RecordingHook {
            events: Rc<RefCell<Vec<String>>>,
        }

        impl RunnerHook for RecordingHook {
            fn on_benchmark_start(&mut self, path: &BenchmarkPath) {
                self.events
                    .borrow_mut()
                    .push(format!("benchmark:{}", path.parts().join(".")));
            }

            fn on_case_start(&mut self, case: &Case) {
                self.events
                    .borrow_mut()
                    .push(format!("case:{}", case.input_required("rows").unwrap()));
            }

            fn on_case_finish(&mut self, _case: &Case, result: &RunResult) {
                self.events
                    .borrow_mut()
                    .push(format!("samples:{}", result.samples.len()));
            }
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let runner = Runner::new(RunnerConfig::new(tiny_config()));

        runner
            .suite("examples", |suite| {
                suite.benchmark("sum", |benchmark| {
                    benchmark
                        .input("rows", InputRange::values([1]))
                        .variant("v", noop_case)
                })
            })
            .hook(RecordingHook {
                events: Rc::clone(&events),
            })
            .run()
            .unwrap();

        assert_eq!(
            events.borrow().as_slice(),
            ["benchmark:examples.sum", "case:1", "samples:2"]
        );
    }

    #[test]
    fn setup_runs_once_and_is_not_sampled() {
        use std::cell::Cell;
        use std::rc::Rc;

        let setup_count = Rc::new(Cell::new(0));
        setup_count.set(setup_count.get() + 1);
        let values = vec![1_u64, 2, 3];
        let workload = MeasurementBuilder::new()
            .measure(move || values.iter().sum::<u64>())
            .build();
        let result = run_adaptive_measurement(&tiny_config(), workload).unwrap();

        assert_eq!(setup_count.get(), 1);
        assert_eq!(result.samples.len(), 2);
    }

    fn noop_case(benchmark: MeasurementBuilder, _case: &Case) -> Result<WorkloadFn> {
        Ok(benchmark.measure(|| 1_u64).build())
    }

    fn sample(ns_per_iter: f64) -> Sample {
        Sample {
            iterations: 10,
            elapsed: Duration::from_nanos((ns_per_iter * 10.0) as u64),
            baseline_elapsed: None,
        }
    }

    fn timed_sample(iterations: u64, elapsed_ns: u64) -> Sample {
        Sample {
            iterations,
            elapsed: Duration::from_nanos(elapsed_ns),
            baseline_elapsed: None,
        }
    }

    fn baseline_sample(iterations: u64, elapsed_ns: u64, baseline_elapsed_ns: u64) -> Sample {
        Sample {
            iterations,
            elapsed: Duration::from_nanos(elapsed_ns),
            baseline_elapsed: Some(Duration::from_nanos(baseline_elapsed_ns)),
        }
    }

    fn tiny_config() -> MeasurementConfig {
        let default = MeasurementConfig::default();
        MeasurementConfig::new(
            Duration::from_nanos(1),
            Duration::from_nanos(1),
            default.target_relative_error(),
            2,
            2,
            default.confidence_z(),
        )
        .unwrap()
    }
}
