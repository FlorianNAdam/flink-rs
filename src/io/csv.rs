use crate::Result;
use anyhow::{anyhow, bail};
use std::str::FromStr;

pub(super) struct RowData {
    pub(super) iterations: u64,
    pub(super) elapsed_ns: u64,
    pub(super) baseline_elapsed_ns: String,
}

pub(super) fn parse_csv_row(line: &str) -> Result<RowData> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 4 {
        bail!("expected at least 4 CSV columns, got {}", fields.len());
    }
    Ok(RowData {
        iterations: parse_field(&fields[1], "iterations")?,
        elapsed_ns: parse_field(&fields[2], "elapsed_ns")?,
        baseline_elapsed_ns: fields[3].to_string(),
    })
}

fn parse_field<T: FromStr>(field: &str, name: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    field
        .trim()
        .parse::<T>()
        .map_err(|e| anyhow!("failed to parse CSV column {name}: {e}"))
}
