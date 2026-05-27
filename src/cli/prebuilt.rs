use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;

use crate::cli::{InputOverride, PrettyPrinterHook, RunnerArgs};
use crate::runner::Executor;
use crate::{
    AdaptiveExecutor, BenchmarkPath, BenchmarkResults, FixedExecutor, InputOverrideSpec, Runner,
};

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum ExecutorKind {
    Adaptive,
    Fixed,
}

impl ExecutorKind {
    fn build(self) -> Box<dyn Executor> {
        match self {
            ExecutorKind::Adaptive => Box::new(AdaptiveExecutor),
            ExecutorKind::Fixed => Box::new(FixedExecutor::default()),
        }
    }
}

#[derive(Parser)]
pub struct Cli {
    #[clap(flatten)]
    pub runner: RunnerArgs,

    /// Executor strategy: adaptive or fixed
    #[arg(long, value_enum, default_value_t = ExecutorKind::Adaptive)]
    pub executor: ExecutorKind,

    /// Output directory for benchmark results
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Re-run benchmarks even if results already exist
    #[arg(long)]
    pub rerun: bool,

    /// Only run benchmarks matching the given path(s) (e.g. "examples.sum")
    pub only: Vec<String>,

    /// Input overrides. Format: "bench.path input_name val1,val2,..."
    #[arg(long = "override", value_name = "PATH INPUT VALS")]
    pub overrides: Vec<InputOverride>,

    /// Accepted for compatibility with bench harness conventions
    #[arg(long, hide = true)]
    pub bench: bool,

    /// Accepted for compatibility with bench harness conventions
    #[arg(long, hide = true)]
    pub test: bool,
}

pub fn run(define_runner: impl FnOnce(crate::Runner) -> crate::Runner) -> crate::Result<()> {
    let started = Instant::now();
    let cli = Cli::parse();
    let runner_config = cli.runner.to_runner_config()?;
    let runner = Runner::new(runner_config.clone());

    let overrides: Vec<InputOverrideSpec> = cli.overrides.iter().map(Into::into).collect();

    let out_dir = output_dir(cli.bench, cli.out_dir.as_deref());
    let config = runner_config.measurement();
    let rerun = cli.rerun || crate::io::config_changed(&out_dir, &config);

    let existing = if !rerun && out_dir.exists() {
        crate::io::load_results(&out_dir)?
    } else {
        BenchmarkResults::default()
    };

    let only_paths: Vec<BenchmarkPath> = cli
        .only
        .iter()
        .map(|s| BenchmarkPath::from(s.as_str()))
        .collect();

    let runner = runner
        .executor(cli.executor.build())
        .overrides(overrides)
        .filter(|f| f.filter_rerun(rerun, &existing))
        .filter(|f| f.by_optional_paths(only_paths.clone()))
        .hook(PrettyPrinterHook::new(config));
    let runner = define_runner(runner);
    let outputs = runner.run()?;

    let paths = crate::io::save_results(&out_dir, &outputs, &config)?;
    println!("Wrote {} benchmark results", paths.len());
    println!("Total time: {:?}", started.elapsed());

    Ok(())
}

fn output_dir(bench: bool, override_dir: Option<&Path>) -> PathBuf {
    if let Some(out_dir) = override_dir {
        return out_dir.to_path_buf();
    }

    if !bench {
        return PathBuf::from("eval-results");
    }

    if let Some(path) = non_empty_env_path("FLINK_HOME") {
        return path;
    }

    if let Some(path) = non_empty_env_path("CARGO_TARGET_DIR") {
        return path.join("flink");
    }

    PathBuf::from("target").join("flink")
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dir_uses_explicit_override() {
        assert_eq!(
            output_dir(true, Some(Path::new("custom-results"))),
            PathBuf::from("custom-results")
        );
    }

    #[test]
    fn output_dir_keeps_eval_results_default_outside_bench_mode() {
        assert_eq!(output_dir(false, None), PathBuf::from("eval-results"));
    }
}
