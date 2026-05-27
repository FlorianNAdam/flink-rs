use std::time::Duration;

use flink::{
    Case, InputRange, MeasurementBuilder, MeasurementConfig, Runner, RunnerConfig, WorkloadFn,
};

fn main() -> flink::Result<()> {
    let default = MeasurementConfig::default();
    let runner = Runner::new(RunnerConfig::new(MeasurementConfig::new(
        Duration::from_millis(10),
        Duration::from_millis(5),
        default.target_relative_error(),
        5,
        10,
        default.confidence_z(),
    )?));

    let results = runner
        .suite("collections", |suite| {
            suite.benchmark("sum", |benchmark| {
                benchmark
                    .input("rows", InputRange::values([1_000, 10_000]))
                    .variant("iterator", sum_with_iterator)
                    .variant("loop", sum_with_loop)
            })
        })
        .run()?;

    for output in &results {
        println!(
            "{}: mean={:.3} ns, median={:.3} ns",
            output.id, output.result.stats.mean_ns, output.result.stats.median_ns
        );
    }

    Ok(())
}

fn sum_with_iterator(benchmark: MeasurementBuilder, case: &Case) -> flink::Result<WorkloadFn> {
    let rows = case.input_required("rows")?;
    let values = (0..rows).collect::<Vec<u64>>();
    Ok(benchmark
        .measure(move || values.iter().copied().sum::<u64>())
        .build())
}

fn sum_with_loop(benchmark: MeasurementBuilder, case: &Case) -> flink::Result<WorkloadFn> {
    let rows = case.input_required("rows")?;
    let values = (0..rows).collect::<Vec<u64>>();
    Ok(benchmark
        .measure(move || {
            let mut sum = 0;
            for value in &values {
                sum += *value;
            }
            sum
        })
        .build())
}
