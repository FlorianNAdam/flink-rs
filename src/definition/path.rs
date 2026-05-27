use super::InputRange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputOverrideSpec {
    pub benchmark_path: BenchmarkPath,
    pub input_name: String,
    pub range: InputRange,
}

pub trait IntoBenchmarkPath {
    fn into_benchmark_path(self) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkPath {
    parts: Vec<String>,
}

impl BenchmarkPath {
    pub fn from_parts(parts: Vec<String>) -> Self {
        Self { parts }
    }

    pub fn new(
        suite_path: impl IntoIterator<Item = impl Into<String>>,
        benchmark_name: impl Into<String>,
    ) -> Self {
        let mut parts: Vec<String> = suite_path.into_iter().map(Into::into).collect();
        parts.push(benchmark_name.into());
        Self { parts }
    }

    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn suite(&self) -> &[String] {
        let len = self.parts.len().saturating_sub(1);
        &self.parts[..len]
    }

    pub fn benchmark(&self) -> Option<&str> {
        self.parts.last().map(String::as_str)
    }

    pub fn matches_prefix(&self, path: &BenchmarkPath) -> bool {
        path.parts.starts_with(&self.parts)
    }

    pub fn matches_exact(&self, path: &BenchmarkPath) -> bool {
        self.parts == path.parts
    }
}

impl<T: IntoBenchmarkPath> From<T> for BenchmarkPath {
    fn from(value: T) -> Self {
        Self {
            parts: value.into_benchmark_path(),
        }
    }
}

impl IntoBenchmarkPath for &str {
    fn into_benchmark_path(self) -> Vec<String> {
        self.split('.')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    }
}

impl IntoBenchmarkPath for String {
    fn into_benchmark_path(self) -> Vec<String> {
        self.as_str().into_benchmark_path()
    }
}

impl IntoBenchmarkPath for Vec<String> {
    fn into_benchmark_path(self) -> Vec<String> {
        self
    }
}

impl<const N: usize> IntoBenchmarkPath for [&str; N] {
    fn into_benchmark_path(self) -> Vec<String> {
        self.into_iter().map(str::to_string).collect()
    }
}
