use std::time::{Duration, Instant};

use crate::{BenchmarkPath, BenchmarkResults, Case, MeasurementConfig, RunResult, RunnerHook};

pub struct PrettyPrinterHook {
    config: MeasurementConfig,
    current_case_started: Option<Instant>,
    phase_started: Option<Instant>,
}

impl PrettyPrinterHook {
    pub fn new(config: MeasurementConfig) -> Self {
        Self {
            config,
            current_case_started: None,
            phase_started: None,
        }
    }
}

impl RunnerHook for PrettyPrinterHook {
    fn on_benchmark_start(&mut self, path: &BenchmarkPath) {
        println!("{}{}{}", ANSI_BOLD_CYAN, path.parts().join("."), ANSI_RESET);
    }

    fn on_benchmark_skip(&mut self, path: &BenchmarkPath) {
        println!(
            "{}{}{} {}skipped{}",
            ANSI_BOLD_CYAN,
            path.parts().join("."),
            ANSI_RESET,
            ANSI_DIM,
            ANSI_RESET,
        );
    }

    fn on_benchmark_finish(&mut self, path: &BenchmarkPath, _results: &BenchmarkResults) {
        println!()
    }

    fn on_set_start(&mut self, cases: &[Case]) {
        if let Some(case) = cases.first() {
            println!("  {}{}{}", ANSI_BOLD, case.id().input.as_str(), ANSI_RESET);
        }
    }

    fn on_set_warmup_start(&mut self, _cases: &[Case]) {
        println!(
            "    {:<12} {}{}{}",
            "warm-up:",
            ANSI_DIM,
            format_duration(self.config.warmup_time()),
            ANSI_RESET,
        );
    }

    fn on_set_calibration_start(&mut self, cases: &[Case]) {
        self.phase_started = Some(Instant::now());
    }

    fn on_set_calibration_finish(&mut self, iterations_per_sample: u64) {
        if let Some(elapsed) = self.phase_started.take().map(|t| t.elapsed()) {
            println!(
                "    {:<12}{} {}/sample, took {}{}",
                "calibration:",
                ANSI_DIM,
                iterations_per_sample,
                format_duration(elapsed),
                ANSI_RESET
            );
        }
    }

    fn on_case_start(&mut self, case: &Case) {
        self.current_case_started = Some(Instant::now());
        println!("    {}{}{}", ANSI_BOLD, case_label(case), ANSI_RESET);
    }

    fn on_case_warmup_start(&mut self, _case: &Case) {
        println!(
            "      {:<12} {}{}{}",
            "warm-up:",
            ANSI_DIM,
            format_duration(self.config.warmup_time()),
            ANSI_RESET,
        );
    }

    fn on_case_calibration_start(&mut self, _case: &Case) {
        self.phase_started = Some(Instant::now());
    }

    fn on_case_calibration_finish(&mut self, _case: &Case, iterations_per_sample: u64) {
        if let Some(elapsed) = self.phase_started.take().map(|t| t.elapsed()) {
            println!(
                "      {:<12}{} {}/sample, took {}{}",
                "calibration:",
                ANSI_DIM,
                iterations_per_sample,
                format_duration(elapsed),
                ANSI_RESET
            );
        }
    }

    fn on_case_collection_start(
        &mut self,
        _case: &Case,
        iterations_per_sample: u64,
        _samples: usize,
    ) {
        println!(
            "      {:<12} {}{}, {} per sample{}",
            "collection:",
            ANSI_DIM,
            format_duration(self.config.min_sample_time()),
            iterations_per_sample,
            ANSI_RESET,
        );
    }

    fn on_case_finish(&mut self, _case: &Case, result: &RunResult) {
        let elapsed = self
            .current_case_started
            .take()
            .map(|started| started.elapsed());
        let min_iterations = result
            .samples
            .iter()
            .map(|sample| sample.iterations)
            .min()
            .unwrap_or(result.iterations_per_sample);
        let max_iterations = result
            .samples
            .iter()
            .map(|sample| sample.iterations)
            .max()
            .unwrap_or(result.iterations_per_sample);
        println!(
            "      {:<12} {}{}, iterations/sample: {}-{}, elapsed: {}{}",
            "samples:",
            ANSI_DIM,
            result.samples.len(),
            min_iterations,
            max_iterations,
            elapsed
                .map(format_duration)
                .unwrap_or_else(|| "unknown".into()),
            ANSI_RESET,
        );
        println!(
            "      {:<12} [{}{:>10}{} {}{:>10}{} {}{:>10}{}]",
            "time:",
            ANSI_DIM,
            format_ns(result.stats.lower_bound_ns),
            ANSI_RESET,
            ANSI_BOLD,
            format_ns(result.stats.mean_ns),
            ANSI_RESET,
            ANSI_DIM,
            format_ns(result.stats.upper_bound_ns),
            ANSI_RESET,
        );
    }
}

fn case_label(case: &Case) -> String {
    let id = case.id();
    let mut label = String::new();
    label.push_str(&id.variant);
    label.push('/');
    label.push_str(id.input.as_str());
    label
}

fn format_ns(ns: f64) -> String {
    if ns >= 1_000_000_000.0 {
        format!("{:.3} s", ns / 1_000_000_000.0)
    } else if ns >= 1_000_000.0 {
        format!("{:.3} ms", ns / 1_000_000.0)
    } else if ns >= 1_000.0 {
        format!("{:.3} µs", ns / 1_000.0)
    } else {
        format!("{:.3} ns", ns)
    }
}

fn format_duration(duration: Duration) -> String {
    format_ns(duration.as_nanos() as f64)
}

const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_RESET: &str = "\x1b[0m";
