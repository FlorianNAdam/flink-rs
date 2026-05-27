mod executor;
mod filters;
mod hooks;
mod results;

pub use executor::{AdaptiveExecutor, Executor, FixedExecutor};
pub use filters::{
    FilterByPaths, FilterByPathsExact, FilterInverse, FilterPredicateBuilder, Predicate,
};
pub use hooks::RunnerHook;
pub use results::{BenchmarkResult, BenchmarkResults};

use crate::Result;
use crate::definition::{Benchmark, BenchmarkPath, BenchmarkSuite, Case, InputOverrideSpec};
use crate::measurement::{MeasurementConfig, RunResult, Sample};
use anyhow::ensure;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    measurement: MeasurementConfig,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            measurement: MeasurementConfig::default(),
        }
    }
}

impl RunnerConfig {
    pub fn new(measurement: MeasurementConfig) -> Self {
        Self { measurement }
    }

    pub fn measurement(&self) -> MeasurementConfig {
        self.measurement
    }
}

pub struct Runner {
    config: RunnerConfig,
    root_suite: BenchmarkSuite,
    overrides: Vec<InputOverrideSpec>,
    predicates: Vec<Box<dyn Predicate>>,
    hooks: RefCell<Vec<Box<dyn RunnerHook>>>,
    executor: Box<dyn Executor>,
}

impl Runner {
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            config,
            root_suite: BenchmarkSuite::new(""),
            overrides: Vec::new(),
            predicates: Vec::new(),
            hooks: RefCell::new(Vec::new()),
            executor: Box::new(AdaptiveExecutor),
        }
    }

    pub fn benchmark(
        mut self,
        name: impl Into<String>,
        define: impl FnOnce(Benchmark) -> Benchmark,
    ) -> Self {
        self.root_suite = self.root_suite.benchmark(name, define);
        self
    }

    pub fn suite(
        mut self,
        name: impl Into<String>,
        define: impl FnOnce(BenchmarkSuite) -> BenchmarkSuite,
    ) -> Self {
        self.root_suite = self.root_suite.suite(name, define);
        self
    }

    pub fn root_suite(mut self, define: impl FnOnce(BenchmarkSuite) -> BenchmarkSuite) -> Self {
        self.root_suite = define(BenchmarkSuite::new(""));
        self
    }

    pub fn override_input(
        mut self,
        benchmark_path: impl crate::IntoBenchmarkPath,
        input_name: impl Into<String>,
        range: crate::InputRange,
    ) -> Self {
        self.overrides.push(InputOverrideSpec {
            benchmark_path: BenchmarkPath::from(benchmark_path),
            input_name: input_name.into(),
            range,
        });
        self
    }

    pub fn overrides(mut self, specs: impl IntoIterator<Item = InputOverrideSpec>) -> Self {
        self.overrides.extend(specs);
        self
    }

    pub fn filter(
        mut self,
        filter: impl FnOnce(FilterPredicateBuilder) -> FilterPredicateBuilder,
    ) -> Self {
        self.predicates
            .extend(filter(FilterPredicateBuilder::new()).predicates);
        self
    }

    pub fn hook(mut self, hook: impl RunnerHook + 'static) -> Self {
        self.hooks.get_mut().push(Box::new(hook));
        self
    }

    pub fn executor(mut self, executor: impl Executor + 'static) -> Self {
        self.executor = Box::new(executor);
        self
    }

    pub fn run(&self) -> Result<BenchmarkResults> {
        self.suite_inner(&self.root_suite)
    }

    fn suite_inner(&self, suite: &BenchmarkSuite) -> Result<BenchmarkResults> {
        let mut outputs = BenchmarkResults::new();
        self.notify(|hook| hook.on_run_start());
        self.run_suite_inner(suite, &mut Vec::new(), &mut outputs)?;
        self.notify(|hook| hook.on_run_finish(&outputs));
        Ok(outputs)
    }

    fn run_suite_inner(
        &self,
        suite: &BenchmarkSuite,
        path: &mut Vec<String>,
        outputs: &mut BenchmarkResults,
    ) -> Result<()> {
        if !suite.name.is_empty() {
            path.push(suite.name.clone());
            self.notify(|hook| hook.on_suite_start(path));
        }
        for benchmark in &suite.benchmarks {
            let benchmark_path = BenchmarkPath::new(path.iter().cloned(), benchmark.name.clone());
            let pass = self.predicates.is_empty()
                || self.predicates.iter().all(|p| p.matches(&benchmark_path));
            if pass {
                let results = self.run_benchmark(benchmark, &benchmark_path)?;
                outputs.extend(results);
            } else {
                self.notify(|hook| hook.on_benchmark_skip(&benchmark_path));
            }
        }
        for suite in &suite.suites {
            self.run_suite_inner(suite, path, outputs)?;
        }
        if !suite.name.is_empty() {
            self.notify(|hook| hook.on_suite_finish(&path));
            path.pop();
        }
        Ok(())
    }

    fn run_benchmark(
        &self,
        benchmark: &Benchmark,
        benchmark_path: &BenchmarkPath,
    ) -> Result<BenchmarkResults> {
        let mut benchmark_overrides = crate::InputOverrides::new();
        for override_spec in &self.overrides {
            if &override_spec.benchmark_path == benchmark_path {
                benchmark_overrides.insert(
                    override_spec.input_name.clone(),
                    override_spec.range.clone(),
                );
            }
        }
        let mut outputs = BenchmarkResults::new();
        self.notify(|hook| hook.on_benchmark_start(benchmark_path));

        benchmark.ensure_has_variants()?;
        for input_set in benchmark.expand_input_sets(&benchmark_overrides)? {
            let cases = benchmark
                .cases(benchmark_path, &input_set)
                .collect::<Vec<_>>();

            self.notify(|hook| hook.on_set_start(&cases));
            let results = {
                let mut hooks = self.hooks.borrow_mut();
                self.executor
                    .run_set(benchmark, &cases, &self.config.measurement, &mut *hooks)?
            };
            self.notify(|hook| hook.on_set_finish(&cases));

            ensure!(
                results.len() == cases.len(),
                "executor returned {} results for {} cases",
                results.len(),
                cases.len(),
            );

            for (case, result) in cases.iter().zip(results) {
                outputs.push(BenchmarkResult {
                    id: case.id(),
                    result,
                });
            }
        }

        self.notify(|hook| hook.on_benchmark_finish(benchmark_path, &outputs));
        Ok(outputs)
    }

    fn notify(&self, mut notify: impl FnMut(&mut dyn RunnerHook)) {
        for hook in self.hooks.borrow_mut().iter_mut() {
            notify(hook.as_mut());
        }
    }
}

impl RunnerHook for Vec<Box<dyn RunnerHook>> {
    fn on_set_start(&mut self, cases: &[Case]) {
        for hook in self.iter_mut() {
            hook.on_set_start(cases);
        }
    }

    fn on_set_finish(&mut self, cases: &[Case]) {
        for hook in self.iter_mut() {
            hook.on_set_finish(cases);
        }
    }

    fn on_set_calibration_start(&mut self, cases: &[Case]) {
        for hook in self.iter_mut() {
            hook.on_set_calibration_start(cases);
        }
    }

    fn on_set_calibration_finish(&mut self, iterations_per_sample: u64) {
        for hook in self.iter_mut() {
            hook.on_set_calibration_finish(iterations_per_sample);
        }
    }

    fn on_set_warmup_start(&mut self, cases: &[Case]) {
        for hook in self.iter_mut() {
            hook.on_set_warmup_start(cases);
        }
    }

    fn on_set_warmup_finish(&mut self, iterations_per_sample: u64) {
        for hook in self.iter_mut() {
            hook.on_set_warmup_finish(iterations_per_sample);
        }
    }

    fn on_case_start(&mut self, case: &Case) {
        for hook in self.iter_mut() {
            hook.on_case_start(case);
        }
    }

    fn on_case_warmup_start(&mut self, case: &Case) {
        for hook in self.iter_mut() {
            hook.on_case_warmup_start(case);
        }
    }

    fn on_case_warmup_finish(&mut self, case: &Case) {
        for hook in self.iter_mut() {
            hook.on_case_warmup_finish(case);
        }
    }

    fn on_case_calibration_start(&mut self, case: &Case) {
        for hook in self.iter_mut() {
            hook.on_case_calibration_start(case);
        }
    }

    fn on_case_calibration_finish(&mut self, case: &Case, iterations_per_sample: u64) {
        for hook in self.iter_mut() {
            hook.on_case_calibration_finish(case, iterations_per_sample);
        }
    }

    fn on_case_collection_start(
        &mut self,
        case: &Case,
        iterations_per_sample: u64,
        samples: usize,
    ) {
        for hook in self.iter_mut() {
            hook.on_case_collection_start(case, iterations_per_sample, samples);
        }
    }

    fn on_case_collection_sample(&mut self, case: &Case, sample_index: usize, sample: &Sample) {
        for hook in self.iter_mut() {
            hook.on_case_collection_sample(case, sample_index, sample);
        }
    }

    fn on_case_collection_finish(&mut self, case: &Case, result: &RunResult) {
        for hook in self.iter_mut() {
            hook.on_case_collection_finish(case, result);
        }
    }

    fn on_case_finish(&mut self, case: &Case, result: &RunResult) {
        for hook in self.iter_mut() {
            hook.on_case_finish(case, result);
        }
    }
}
