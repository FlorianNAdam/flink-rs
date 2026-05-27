use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use crate::Result;
use crate::definition::CaseId;
use crate::measurement::{MeasurementConfig, RunResult, Sample, Stats};
use crate::runner::{BenchmarkResult, BenchmarkResults};
use crate::util::sanitize_path_part;
use anyhow::{Context, anyhow, bail};

use self::csv::parse_csv_row;

mod csv;

pub fn output_path(out_dir: &Path, id: &CaseId) -> PathBuf {
    let mut path = out_dir.to_path_buf().join("benchmarks");
    for suite in &id.suite {
        path = path.join(sanitize_path_part(suite));
    }
    path.join(sanitize_path_part(&id.benchmark))
        .join(sanitize_path_part(&id.variant))
        .join(id.file_name())
}

fn config_path(out_dir: &Path) -> PathBuf {
    out_dir.join("config.json")
}

fn write_json_config(path: &Path, config: &MeasurementConfig) -> Result<()> {
    let json = format!(
        "{{\n  \"warmup_ms\": {},\n  \"min_sample_ms\": {},\n  \"target_relative_error\": {},\n  \"min_samples\": {},\n  \"max_samples\": {},\n  \"confidence_z\": {}\n}}\n",
        config.warmup_time().as_millis(),
        config.min_sample_time().as_millis(),
        config.target_relative_error(),
        config.min_samples(),
        config.max_samples(),
        config.confidence_z(),
    );
    fs::write(path, json)?;
    Ok(())
}

fn read_json_config(path: &Path) -> Result<MeasurementConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let trimmed = content.trim();
    let inner = trimmed.trim_start_matches('{').trim_end_matches('}').trim();
    let mut warmup_ms = None;
    let mut min_sample_ms = None;
    let mut target_relative_error = None;
    let mut min_samples = None;
    let mut max_samples = None;
    let mut confidence_z = None;
    for pair in inner.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, ':');
        let key = parts.next().unwrap().trim().trim_matches('"');
        let value = parts.next().unwrap().trim();
        match key {
            "warmup_ms" => {
                warmup_ms = Some(
                    value
                        .parse()
                        .with_context(|| format!("invalid warmup_ms value: {value}"))?,
                );
            }
            "min_sample_ms" => {
                min_sample_ms = Some(
                    value
                        .parse()
                        .with_context(|| format!("invalid min_sample_ms value: {value}"))?,
                );
            }
            "target_relative_error" => {
                target_relative_error =
                    Some(value.parse::<f64>().with_context(|| {
                        format!("invalid target_relative_error value: {value}")
                    })?);
            }
            "min_samples" => {
                min_samples = Some(
                    value
                        .parse()
                        .with_context(|| format!("invalid min_samples value: {value}"))?,
                );
            }
            "max_samples" => {
                max_samples = Some(
                    value
                        .parse()
                        .with_context(|| format!("invalid max_samples value: {value}"))?,
                );
            }
            "confidence_z" => {
                confidence_z = Some(
                    value
                        .parse::<f64>()
                        .with_context(|| format!("invalid confidence_z value: {value}"))?,
                );
            }
            _ => {}
        }
    }
    MeasurementConfig::new(
        Duration::from_millis(
            warmup_ms.ok_or_else(|| anyhow!("missing warmup_ms in {}", path.display()))?,
        ),
        Duration::from_millis(
            min_sample_ms.ok_or_else(|| anyhow!("missing min_sample_ms in {}", path.display()))?,
        ),
        target_relative_error
            .ok_or_else(|| anyhow!("missing target_relative_error in {}", path.display()))?,
        min_samples.ok_or_else(|| anyhow!("missing min_samples in {}", path.display()))?,
        max_samples.ok_or_else(|| anyhow!("missing max_samples in {}", path.display()))?,
        confidence_z.ok_or_else(|| anyhow!("missing confidence_z in {}", path.display()))?,
    )
}

pub fn save_results(
    out_dir: &Path,
    results: &BenchmarkResults,
    config: &MeasurementConfig,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out_dir)?;
    write_json_config(&config_path(out_dir), config)?;
    results
        .into_iter()
        .map(|result| save_result_csv(out_dir, result))
        .collect()
}

fn save_result_csv(out_dir: &Path, output: &BenchmarkResult) -> Result<PathBuf> {
    let path = output_path(out_dir, &output.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "sample,iterations,elapsed_ns,baseline_elapsed_ns")?;
    for (sample_index, sample) in output.result.samples.iter().enumerate() {
        let baseline_ns = sample
            .baseline_elapsed
            .map(|elapsed| elapsed.as_nanos().to_string())
            .unwrap_or_default();
        writeln!(
            writer,
            "{},{},{},{}",
            sample_index,
            sample.iterations,
            sample.elapsed.as_nanos(),
            baseline_ns,
        )?;
    }
    Ok(path)
}

fn id_from_path(path: &Path) -> Result<CaseId> {
    let components: Vec<&str> = path.iter().filter_map(|c| c.to_str()).collect();
    let bench_idx = components
        .iter()
        .position(|c| *c == "benchmarks")
        .ok_or_else(|| anyhow!("path does not contain benchmarks dir: {}", path.display()))?;
    let parts = &components[bench_idx + 1..];
    if parts.len() < 3 {
        bail!(
            "path too short to extract benchmark identity: {}",
            path.display()
        );
    }
    let input_file = parts.last().unwrap();
    let input_label = input_file.strip_suffix(".csv").unwrap_or(input_file);
    let variant = parts[parts.len() - 2];
    let benchmark = parts[parts.len() - 3];
    let suite: Vec<String> = parts[..parts.len() - 3]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    Ok(CaseId::new(benchmark, variant, input_label).with_suite(suite))
}

fn load_result(path: &Path, config: &MeasurementConfig) -> Result<BenchmarkResult> {
    let id = id_from_path(path)?;

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let _header = lines
        .next()
        .ok_or_else(|| anyhow!("empty CSV: {}", path.display()))??;

    let mut samples = Vec::new();

    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row = parse_csv_row(&line)
            .with_context(|| format!("failed to parse CSV row in {}", path.display()))?;
        let baseline = if row.baseline_elapsed_ns.is_empty() {
            None
        } else {
            Some(Duration::from_nanos(
                u64::from_str(&row.baseline_elapsed_ns).with_context(|| {
                    format!("invalid baseline_elapsed_ns in {}", path.display())
                })?,
            ))
        };
        samples.push(Sample {
            iterations: row.iterations,
            elapsed: Duration::from_nanos(row.elapsed_ns),
            baseline_elapsed: baseline,
        });
    }

    if samples.is_empty() {
        bail!("no data in CSV: {}", path.display());
    }

    let stats = Stats::from_samples(&samples, config.confidence_z());
    let iterations_per_sample = samples[0].iterations;

    let result = RunResult {
        iterations_per_sample,
        required_samples: samples.len(),
        samples,
        stats,
        confidence_interval_is_heuristic: true,
    };

    Ok(BenchmarkResult { id, result })
}

pub fn config_changed(out_dir: &Path, config: &MeasurementConfig) -> bool {
    let path = config_path(out_dir);
    let Ok(existing) = read_json_config(&path) else {
        return true;
    };
    existing != *config
}

pub fn load_results(out_dir: &Path) -> Result<BenchmarkResults> {
    let config = read_json_config(&config_path(out_dir))?;
    let mut results = BenchmarkResults::new();
    collect_csv_files(out_dir, &config, &mut results)?;
    Ok(results)
}

fn collect_csv_files(
    dir: &Path,
    config: &MeasurementConfig,
    results: &mut BenchmarkResults,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_csv_files(&path, config, results)?;
        } else if path.extension().map_or(false, |ext| ext == "csv") {
            let output = load_result(&path, config)?;
            results.push(output);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaseInput;
    use crate::measurement::{MeasurementBuilder, MeasurementConfig, run_adaptive_measurement};
    use std::time::Duration;

    #[test]
    fn round_trip_save_and_load() {
        let dir = std::env::temp_dir().join(format!("flink-io-test-{}", std::process::id()));
        let default = MeasurementConfig::default();
        let config = MeasurementConfig::new(
            Duration::from_millis(5),
            Duration::from_millis(2),
            default.target_relative_error(),
            2,
            3,
            default.confidence_z(),
        )
        .unwrap();
        let id = CaseId::new("bench", "var", CaseInput::scalar(42u64)).with_suite(["s1", "s2"]);
        let measurement = MeasurementBuilder::new().measure(|| 1u64).build().unwrap();
        let result = run_adaptive_measurement(&config, measurement).unwrap();

        let mut results = BenchmarkResults::new();
        results.push(BenchmarkResult { id, result });

        let paths = save_results(&dir, &results, &config).unwrap();
        let path = &paths[0];
        assert!(path.exists());

        let loaded = load_results(&dir).unwrap();
        assert_eq!(loaded.len(), 1);
        let loaded = &loaded[0];
        assert_eq!(loaded.id.benchmark, "bench");
        assert_eq!(loaded.id.variant, "var");
        assert_eq!(loaded.id.input.as_str(), "42");
        assert_eq!(loaded.id.suite, vec!["s1", "s2"]);
        assert_eq!(loaded.result.samples.len(), results[0].result.samples.len());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
