use std::time::Duration;

use flink::{MeasurementBuilder, MeasurementConfig, run_adaptive_measurement};

fn main() -> flink::Result<()> {
    let default = MeasurementConfig::default();
    let config = MeasurementConfig::new(
        Duration::from_millis(10),
        Duration::from_millis(5),
        default.target_relative_error(),
        5,
        10,
        default.confidence_z(),
    )?;
    let values = (0..10_000).collect::<Vec<u64>>();
    let measurement = MeasurementBuilder::new()
        .measure(move || values.iter().copied().sum::<u64>())
        .build()?;
    let result = run_adaptive_measurement(&config, measurement)?;

    println!(
        "mean={:.3} ns, median={:.3} ns, samples={}",
        result.stats.mean_ns,
        result.stats.median_ns,
        result.samples.len()
    );

    Ok(())
}
