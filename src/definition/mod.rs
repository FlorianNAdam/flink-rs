use crate::Result;
use crate::measurement::{Measurement, MeasurementBuilder, MeasurementConfig, RunResult};
use crate::util::sanitize_path_part;
use anyhow::{anyhow, bail, ensure};
use std::collections::BTreeMap;
use std::fmt;

mod path;

pub use path::{BenchmarkPath, InputOverrideSpec, IntoBenchmarkPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseInput {
    label: String,
}

impl CaseInput {
    pub fn scalar(value: impl ToString) -> Self {
        Self {
            label: value.to_string(),
        }
    }

    pub fn fields(fields: impl IntoIterator<Item = (impl ToString, impl ToString)>) -> Self {
        Self {
            label: fields
                .into_iter()
                .map(|(name, value)| format!("{}={}", name.to_string(), value.to_string()))
                .collect::<Vec<_>>()
                .join("_"),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.label
    }
}

impl fmt::Display for CaseInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.label.fmt(f)
    }
}

impl From<u64> for CaseInput {
    fn from(value: u64) -> Self {
        Self::scalar(value)
    }
}
impl From<usize> for CaseInput {
    fn from(value: usize) -> Self {
        Self::scalar(value)
    }
}
impl From<i32> for CaseInput {
    fn from(value: i32) -> Self {
        Self::scalar(value)
    }
}
impl From<u32> for CaseInput {
    fn from(value: u32) -> Self {
        Self::scalar(value)
    }
}
impl From<String> for CaseInput {
    fn from(value: String) -> Self {
        Self { label: value }
    }
}
impl From<&str> for CaseInput {
    fn from(value: &str) -> Self {
        Self {
            label: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseId {
    pub suite: Vec<String>,
    pub benchmark: String,
    pub variant: String,
    pub input: CaseInput,
}

impl CaseId {
    pub fn new(
        benchmark: impl Into<String>,
        variant: impl Into<String>,
        input: impl Into<CaseInput>,
    ) -> Self {
        Self {
            suite: Vec::new(),
            benchmark: benchmark.into(),
            variant: variant.into(),
            input: input.into(),
        }
    }

    pub fn with_suite(mut self, suite: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.suite = suite.into_iter().map(Into::into).collect();
        self
    }

    pub fn file_name(&self) -> String {
        format!("{}.csv", sanitize_path_part(self.input.as_str()))
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.suite.is_empty() {
            write!(f, "suite={}, ", self.suite.join("/"))?;
        }
        write!(
            f,
            "benchmark={}, variant={}, input={}",
            self.benchmark, self.variant, self.input
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRange {
    Values(Vec<u64>),
    Geometric {
        start: u64,
        end: u64,
        multiplier: u64,
    },
}

impl Into<InputRange> for &[u64] {
    fn into(self) -> InputRange {
        InputRange::Values(self.to_vec())
    }
}

impl InputRange {
    pub fn values(values: impl IntoIterator<Item = u64>) -> Self {
        Self::Values(values.into_iter().collect())
    }

    pub fn geometric(start: u64, end: u64, multiplier: u64) -> Self {
        Self::Geometric {
            start,
            end,
            multiplier,
        }
    }

    pub fn expand(&self) -> Result<Vec<u64>> {
        match self {
            Self::Values(values) => {
                ensure!(!values.is_empty(), "input values must not be empty");
                Ok(values.clone())
            }
            Self::Geometric {
                start,
                end,
                multiplier,
            } => {
                ensure!(
                    *start > 0,
                    "geometric input start must be greater than zero"
                );
                ensure!(*end >= *start, "geometric input end must be >= start");
                ensure!(
                    *multiplier > 1,
                    "geometric input multiplier must be greater than one"
                );
                let mut values = Vec::new();
                let mut value = *start;
                while value <= *end {
                    values.push(value);
                    value = value
                        .checked_mul(*multiplier)
                        .ok_or_else(|| anyhow!("geometric input range overflow"))?;
                }
                Ok(values)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDimension {
    pub name: String,
    pub default: InputRange,
}

impl InputDimension {
    pub fn new(name: impl Into<String>, default: InputRange) -> Self {
        Self {
            name: name.into(),
            default,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputOverrides {
    pub(crate) ranges: BTreeMap<String, InputRange>,
}

impl InputOverrides {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(mut self, name: impl Into<String>, range: InputRange) -> Self {
        self.ranges.insert(name.into(), range);
        self
    }
    pub fn insert(&mut self, name: impl Into<String>, range: InputRange) {
        self.ranges.insert(name.into(), range);
    }
    pub(crate) fn get(&self, name: &str) -> Option<&InputRange> {
        self.ranges.get(name)
    }

    pub(crate) fn validate_against(&self, definition: &Benchmark) -> Result<()> {
        for name in self.ranges.keys() {
            if !definition.inputs.iter().any(|input| input.name == *name) {
                bail!(
                    "unknown input override for benchmark {}: {}",
                    definition.name,
                    name
                );
            }
        }
        Ok(())
    }
}

pub struct Benchmark {
    pub name: String,
    pub inputs: Vec<InputDimension>,
    variants: Vec<BenchmarkVariant>,
}

type VariantFn = dyn Fn(MeasurementBuilder, &Case) -> Result<Measurement> + 'static;

pub struct BenchmarkVariant {
    name: String,
    run: Box<VariantFn>,
}

impl BenchmarkVariant {
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Benchmark {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inputs: Vec::new(),
            variants: Vec::new(),
        }
    }

    pub fn input(mut self, name: impl Into<String>, default: impl Into<InputRange>) -> Self {
        self.inputs.push(InputDimension::new(name, default.into()));
        self
    }
    pub fn input_dimension(mut self, input: InputDimension) -> Self {
        self.inputs.push(input);
        self
    }

    pub fn variant(
        mut self,
        name: impl Into<String>,
        run: impl Fn(MeasurementBuilder, &Case) -> Result<Measurement> + 'static,
    ) -> Self {
        self.variants.push(BenchmarkVariant {
            name: name.into(),
            run: Box::new(run),
        });
        self
    }

    pub fn variant_names(&self) -> impl Iterator<Item = &str> {
        self.variants.iter().map(|variant| variant.name.as_str())
    }
    pub fn variant_for_case(&self, case: &Case) -> Option<&BenchmarkVariant> {
        self.variants
            .iter()
            .find(|variant| variant.name == case.variant)
    }

    pub fn cases(
        &self,
        benchmark_path: &BenchmarkPath,
        input_set: &BTreeMap<String, u64>,
    ) -> impl Iterator<Item = Case> {
        self.variant_names().map(|variant| Case {
            benchmark_path: benchmark_path.clone(),
            variant: variant.to_string(),
            inputs: input_set.clone(),
        })
    }
    pub fn measure_case(&self, case: &Case) -> Result<Measurement> {
        let variant = self.variant_for_case(case).ok_or_else(|| {
            anyhow!(
                "unknown variant for benchmark {}: {}",
                self.name,
                case.variant
            )
        })?;
        (variant.run)(MeasurementBuilder::new(), case)
    }

    pub fn variants(
        mut self,
        variants: impl IntoIterator<
            Item = (
                impl Into<String>,
                impl Fn(MeasurementBuilder, &Case) -> Result<Measurement> + 'static,
            ),
        >,
    ) -> Self {
        for (name, run) in variants {
            self = self.variant(name, run);
        }
        self
    }

    pub fn expand_cases(&self, overrides: &InputOverrides) -> Result<Vec<Case>> {
        self.ensure_has_variants()?;
        let input_sets = self.expand_input_sets(overrides)?;
        let mut cases = Vec::new();
        for input_values in input_sets {
            for variant in &self.variants {
                cases.push(Case {
                    benchmark_path: BenchmarkPath::new(Vec::<String>::new(), self.name.clone()),
                    variant: variant.name.clone(),
                    inputs: input_values.clone(),
                });
            }
        }
        Ok(cases)
    }

    pub(crate) fn ensure_has_variants(&self) -> Result<()> {
        ensure!(
            !self.variants.is_empty(),
            "benchmark must define at least one variant"
        );
        Ok(())
    }

    pub(crate) fn expand_input_sets(
        &self,
        overrides: &InputOverrides,
    ) -> Result<Vec<BTreeMap<String, u64>>> {
        overrides.validate_against(self)?;
        let mut sets = vec![BTreeMap::new()];
        for input in &self.inputs {
            let values = overrides
                .get(&input.name)
                .unwrap_or(&input.default)
                .expand()?;
            let mut next_sets = Vec::with_capacity(sets.len() * values.len());
            for set in &sets {
                for value in &values {
                    let mut next_set = set.clone();
                    next_set.insert(input.name.clone(), *value);
                    next_sets.push(next_set);
                }
            }
            sets = next_sets;
        }
        Ok(sets)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub benchmark_path: BenchmarkPath,
    pub variant: String,
    pub inputs: BTreeMap<String, u64>,
}

impl Case {
    pub fn id(&self) -> CaseId {
        CaseId::new(
            self.benchmark_path.benchmark().unwrap_or_default(),
            self.variant.clone(),
            CaseInput::fields(self.inputs.iter()),
        )
        .with_suite(self.benchmark_path.suite().iter().cloned())
    }

    pub fn input(&self, name: &str) -> Option<u64> {
        self.inputs.get(name).copied()
    }
    pub fn input_required(&self, name: &str) -> Result<u64> {
        self.input(name)
            .ok_or_else(|| anyhow!("missing input for case {}: {}", self.id(), name))
    }
}

pub struct CaseRunner {
    pub id: CaseId,
    run: Box<dyn FnMut(&MeasurementConfig) -> Result<RunResult>>,
}

impl CaseRunner {
    pub fn new(
        id: CaseId,
        run: impl FnMut(&MeasurementConfig) -> Result<RunResult> + 'static,
    ) -> Self {
        Self {
            id,
            run: Box::new(run),
        }
    }

    pub fn run(&mut self, config: &MeasurementConfig) -> Result<RunResult> {
        (self.run)(config)
    }
}

pub struct BenchmarkSuite {
    pub name: String,
    pub benchmarks: Vec<Benchmark>,
    pub suites: Vec<BenchmarkSuite>,
}

impl BenchmarkSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            benchmarks: Vec::new(),
            suites: Vec::new(),
        }
    }

    pub fn benchmark(
        mut self,
        name: impl Into<String>,
        define: impl FnOnce(Benchmark) -> Benchmark,
    ) -> Self {
        self.benchmarks.push(define(Benchmark::new(name)));
        self
    }

    pub fn suite(
        mut self,
        name: impl Into<String>,
        define: impl FnOnce(BenchmarkSuite) -> BenchmarkSuite,
    ) -> Self {
        self.suites.push(define(BenchmarkSuite::new(name)));
        self
    }
}
