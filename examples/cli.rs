use flink::cli::prebuilt;
use std::thread::sleep;
use std::time::Duration;

use flink::{Case, InputRange, MeasurementBuilder, WorkloadFn};

fn main() -> flink::Result<()> {
    prebuilt::run(|runner| {
        runner
            .benchmark("sum", |benchmark| {
                benchmark
                    .input("rows", InputRange::values([1_000, 10_000]))
                    .variant("std_iter", sum_std_iter)
            })
            .benchmark("matrix", |benchmark| {
                benchmark
                    .input("rows", InputRange::values([100, 1000]))
                    .input("cols", InputRange::values([100, 500]))
                    .variant("nested_loop", matrix_nested_loop)
                    .variant("flat_iter", matrix_flat_iter)
            })
            .suite("nested", |suite| {
                suite
                    .benchmark("sum_small", |benchmark| {
                        benchmark
                            .input("rows", InputRange::values([100]))
                            .variant("std_iter", sum_std_iter)
                    })
                    .benchmark("sleep", |benchmark| benchmark.variant("std", sleep_std))
            })
    })
}

fn sum_std_iter(measurement: MeasurementBuilder, case: &Case) -> flink::Result<WorkloadFn> {
    let input = case.input_required("rows")?;
    let values = (0..input).collect::<Vec<u64>>();
    Ok(measurement
        .measure(move || values.iter().copied().sum::<u64>())
        .build())
}

fn matrix_nested_loop(measurement: MeasurementBuilder, case: &Case) -> flink::Result<WorkloadFn> {
    let rows = case.input_required("rows")? as usize;
    let cols = case.input_required("cols")? as usize;
    let matrix = vec![vec![1u64; cols]; rows];
    Ok(measurement
        .measure(move || {
            let mut sum = 0u64;
            for row in &matrix {
                for val in row {
                    sum += *val;
                }
            }
            sum
        })
        .build())
}

fn matrix_flat_iter(measurement: MeasurementBuilder, case: &Case) -> flink::Result<WorkloadFn> {
    let rows = case.input_required("rows")? as usize;
    let cols = case.input_required("cols")? as usize;
    let matrix = vec![vec![1u64; cols]; rows];
    Ok(measurement
        .measure(move || matrix.iter().flat_map(|r| r.iter()).sum::<u64>())
        .build())
}

fn sleep_std(measurement: MeasurementBuilder, _case: &Case) -> flink::Result<WorkloadFn> {
    Ok(measurement
        .measure(|| sleep(Duration::from_micros(155)))
        .baseline(|| sleep(Duration::from_nanos(1)))
        .build())
}
