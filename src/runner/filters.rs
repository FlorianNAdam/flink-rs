use crate::definition::BenchmarkPath;

use super::BenchmarkResults;

pub trait Predicate {
    fn matches(&self, path: &BenchmarkPath) -> bool;
}

impl<F: Fn(&BenchmarkPath) -> bool> Predicate for F {
    fn matches(&self, path: &BenchmarkPath) -> bool {
        self(path)
    }
}

pub struct FilterByPathsExact {
    paths: Vec<BenchmarkPath>,
}

impl FilterByPathsExact {
    pub fn new(paths: Vec<BenchmarkPath>) -> Self {
        Self { paths }
    }
}

impl Predicate for FilterByPathsExact {
    fn matches(&self, path: &BenchmarkPath) -> bool {
        self.paths
            .iter()
            .any(|candidate| candidate.matches_exact(path))
    }
}

pub struct FilterByPaths {
    paths: Vec<BenchmarkPath>,
}

impl Predicate for FilterByPaths {
    fn matches(&self, path: &BenchmarkPath) -> bool {
        self.paths
            .iter()
            .any(|candidate| candidate.matches_prefix(path))
    }
}

pub struct FilterInverse {
    inner: Vec<Box<dyn Predicate>>,
}

impl Predicate for FilterInverse {
    fn matches(&self, path: &BenchmarkPath) -> bool {
        !self.inner.iter().any(|p| p.matches(path))
    }
}

pub struct Conditional {
    condition: bool,
    inner: Vec<Box<dyn Predicate>>,
}

impl Predicate for Conditional {
    fn matches(&self, path: &BenchmarkPath) -> bool {
        if self.condition {
            self.inner.iter().any(|p| p.matches(path))
        } else {
            true
        }
    }
}

pub struct FilterPredicateBuilder {
    pub(super) predicates: Vec<Box<dyn Predicate>>,
}

impl FilterPredicateBuilder {
    pub fn new() -> Self {
        Self {
            predicates: Vec::new(),
        }
    }

    pub fn predicate(mut self, predicate: impl Predicate + 'static) -> Self {
        self.predicates.push(Box::new(predicate));
        self
    }

    pub fn by_paths_exact(self, paths: Vec<BenchmarkPath>) -> Self {
        self.predicate(FilterByPathsExact::new(paths))
    }

    pub fn by_paths(self, paths: Vec<BenchmarkPath>) -> Self {
        self.predicate(FilterByPaths { paths })
    }

    pub fn by_optional_paths(self, paths: Vec<BenchmarkPath>) -> Self {
        let empty = paths.is_empty();
        self.conditional(!empty, move |f| f.by_paths(paths))
    }

    pub fn inverse(self, f: impl FnOnce(FilterPredicateBuilder) -> FilterPredicateBuilder) -> Self {
        let inner = f(FilterPredicateBuilder::new()).predicates;
        self.predicate(FilterInverse { inner })
    }

    pub fn conditional(
        self,
        condition: bool,
        f: impl FnOnce(FilterPredicateBuilder) -> FilterPredicateBuilder,
    ) -> Self {
        let inner = f(FilterPredicateBuilder::new()).predicates;
        self.predicate(Conditional { condition, inner })
    }

    pub fn filter_rerun(self, rerun: bool, existing: &BenchmarkResults) -> Self {
        let paths: Vec<BenchmarkPath> = existing
            .into_iter()
            .map(|output| {
                let mut parts = output.id.suite.clone();
                parts.push(output.id.benchmark.clone());
                BenchmarkPath::from_parts(parts)
            })
            .collect();
        self.conditional(!rerun, |f| f.inverse(move |f| f.by_paths_exact(paths)))
    }
}
