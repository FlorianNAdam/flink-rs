use crate::definition::{BenchmarkPath, InputOverrideSpec, InputRange};

#[derive(Debug, Clone)]
pub struct InputOverride {
    pub bench_path: BenchmarkPath,
    pub input_name: String,
    pub values: Vec<u64>,
}

impl std::str::FromStr for InputOverride {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err("expected format: \"bench.path input_name val1,val2,...\"".into());
        }
        let values = parts[2]
            .split(',')
            .map(|v| {
                v.trim()
                    .parse::<u64>()
                    .map_err(|e| format!("invalid value in override: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() {
            return Err("override values must not be empty".into());
        }
        Ok(Self {
            bench_path: BenchmarkPath::from(parts[0]),
            input_name: parts[1].to_string(),
            values,
        })
    }
}

impl From<&InputOverride> for InputOverrideSpec {
    fn from(ov: &InputOverride) -> Self {
        InputOverrideSpec {
            benchmark_path: ov.bench_path.clone(),
            input_name: ov.input_name.clone(),
            range: InputRange::values(ov.values.iter().copied()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_input_override() {
        let ov: InputOverride = "suite.bench input 10,20,30".parse().unwrap();
        assert_eq!(ov.bench_path, BenchmarkPath::from("suite.bench"));
        assert_eq!(ov.input_name, "input");
        assert_eq!(ov.values, vec![10, 20, 30]);
    }

    #[test]
    fn parses_single_override_value() {
        let ov: InputOverride = "bench rows 100".parse().unwrap();
        assert_eq!(ov.bench_path, BenchmarkPath::from("bench"));
        assert_eq!(ov.values, vec![100]);
    }

    #[test]
    fn rejects_empty_override() {
        assert!("bench rows ".parse::<InputOverride>().is_err());
        assert!("".parse::<InputOverride>().is_err());
    }

    #[test]
    fn rejects_missing_parts() {
        assert!("bench rows".parse::<InputOverride>().is_err());
    }

    #[test]
    fn parses_nested_path() {
        let ov: InputOverride = "a.b.c.d rows 1".parse().unwrap();
        assert_eq!(ov.bench_path, BenchmarkPath::from("a.b.c.d"));
    }
}
