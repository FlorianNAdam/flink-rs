use crate::definition::CaseId;
use crate::measurement::RunResult;

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub id: CaseId,
    pub result: RunResult,
}

#[derive(Debug, Clone, Default)]
pub struct BenchmarkResults {
    results: Vec<BenchmarkResult>,
}

impl BenchmarkResults {
    pub(crate) fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    pub(crate) fn extend(&mut self, results: impl IntoIterator<Item = BenchmarkResult>) {
        self.results.extend(results);
    }
}

impl IntoIterator for BenchmarkResults {
    type Item = BenchmarkResult;
    type IntoIter = std::vec::IntoIter<BenchmarkResult>;

    fn into_iter(self) -> Self::IntoIter {
        self.results.into_iter()
    }
}

impl<'a> IntoIterator for &'a BenchmarkResults {
    type Item = &'a BenchmarkResult;
    type IntoIter = std::slice::Iter<'a, BenchmarkResult>;

    fn into_iter(self) -> Self::IntoIter {
        self.results.iter()
    }
}

impl std::ops::Deref for BenchmarkResults {
    type Target = [BenchmarkResult];

    fn deref(&self) -> &Self::Target {
        &self.results
    }
}
