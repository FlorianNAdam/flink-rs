use crate::definition::{BenchmarkPath, Case};
use crate::measurement::{RunResult, Sample};

use super::BenchmarkResults;

#[rustfmt::skip]
pub trait RunnerHook {
    fn on_run_start(&mut self) {}
    fn on_run_finish(&mut self, _results: &BenchmarkResults) {}

    fn on_suite_start(&mut self, _path: &[String]) {}
    fn on_suite_finish(&mut self, _path: &[String]) {}

    fn on_benchmark_start(&mut self, _path: &BenchmarkPath) {}
    fn on_benchmark_finish(&mut self, _path: &BenchmarkPath, _results: &BenchmarkResults) {}
    fn on_benchmark_skip(&mut self, _path: &BenchmarkPath) {}

    fn on_set_start(&mut self, _cases: &[Case]) {}
    fn on_set_finish(&mut self, _cases: &[Case]) {}

    fn on_set_warmup_start(&mut self, _cases: &[Case]) {}
    fn on_set_warmup_finish(&mut self, _iterations_per_sample: u64) {}

    fn on_set_calibration_start(&mut self, _cases: &[Case]) {}
    fn on_set_calibration_finish(&mut self, _iterations_per_sample: u64) {}

    fn on_case_start(&mut self, _case: &Case) {}
    fn on_case_finish(&mut self, _case: &Case, _result: &RunResult) {}

    fn on_case_warmup_start(&mut self, _case: &Case) {}
    fn on_case_warmup_finish(&mut self, _case: &Case) {}

    fn on_case_calibration_start(&mut self, _case: &Case) {}
    fn on_case_calibration_finish(&mut self, _case: &Case, _iterations_per_sample: u64) {}

    fn on_case_collection_start(&mut self, _case: &Case, _iterations_per_sample: u64, _samples: usize) { }
    fn on_case_collection_sample(&mut self, _case: &Case, _sample_index: usize, _sample: &Sample) {}
    fn on_case_collection_finish(&mut self, _case: &Case, _result: &RunResult) {}
}
