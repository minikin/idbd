//! Scenario C24: measured cost of `Idbd::predict` and `Idbd::update`.
//!
//! Times the two core operations separately for three input sizes and both bounds
//! settings, and records the machine, compiler and method beside the numbers.

use std::hint::black_box;
use std::process::{Command, ExitCode};
use std::time::Instant;

use idbd::{Bounds, Config, Idbd};
use idbd_lab::cli::{Invocation, arguments, parse};
use idbd_lab::report::{provenance, write};
use idbd_lab::rng::Rng;
use idbd_lab::stats::{median, percentile};

/// Input sizes measured, as the scenario requires.
const SIZES: [usize; 3] = [20, 1_000, 10_000];

/// Timed batches per configuration. Each contributes one ns-per-call sample.
const SAMPLES: usize = 101;

/// Untimed batches run before the first timed one.
const WARMUP_BATCHES: usize = 3;

/// Inputs that carry signal in the benchmark fixture. The rest are noise.
const RELEVANT: usize = 5;

/// Step size and meta step size of the benchmarked learner.
const INITIAL_STEP_SIZE: f64 = 0.05;
const META_STEP_SIZE: f64 = 0.01;

/// Seed of the benchmark fixture, so the same build measures the same numbers.
const FIXTURE_SEED: u64 = 20_241_102;

const DEFAULT_OUT: &str = "docs";

/// Which core operation a row measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Predict,
    Update,
}

impl Operation {
    const fn name(self) -> &'static str {
        match self {
            Self::Predict => "predict",
            Self::Update => "update",
        }
    }
}

/// One measured configuration.
#[derive(Debug)]
struct Measurement {
    features: usize,
    bounds: &'static str,
    operation: Operation,
    batch: usize,
    nanos: Vec<f64>,
}

impl Measurement {
    fn median(&self) -> Option<f64> {
        median(&self.nanos)
    }

    fn spread(&self) -> Option<(f64, f64)> {
        Some((percentile(&self.nanos, 0.1)?, percentile(&self.nanos, 0.9)?))
    }

    fn per_feature(&self) -> Option<f64> {
        let features = f64::from(u32::try_from(self.features).ok()?);
        Some(self.median()? / features)
    }
}

/// A pool of examples large enough that a timed batch never repeats one.
#[derive(Debug)]
struct Fixture {
    features: Vec<Vec<f64>>,
    targets: Vec<f64>,
}

impl Fixture {
    /// Builds `count` examples of `features` standard normals scaled by
    /// `1/sqrt(features)`, so the squared input norm stays near one at every size and
    /// the learner is stable with the same step size throughout.
    fn new(features: usize, count: usize) -> Self {
        let mut rng = Rng::new(FIXTURE_SEED);
        let scale = 1.0 / f64::from(u32::try_from(features).unwrap_or(u32::MAX)).sqrt();
        let mut inputs = Vec::with_capacity(count);
        let mut targets = Vec::with_capacity(count);
        for _ in 0..count {
            let x: Vec<f64> = (0..features)
                .map(|_| rng.standard_normal() * scale)
                .collect();
            let target: f64 = x.iter().take(RELEVANT).map(|v| v * rng.sign()).sum();
            inputs.push(x);
            targets.push(target);
        }
        Self {
            features: inputs,
            targets,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = match parse(arguments(), DEFAULT_OUT)? {
        Invocation::Help => {
            println!("bench [--out <dir>]\n\ndefault --out {DEFAULT_OUT}, writes benchmarks.md");
            return Ok(());
        }
        Invocation::Run(options) => options,
    };
    if cfg!(debug_assertions) {
        return Err(
            "build with --release; debug assertions make these numbers meaningless".to_owned(),
        );
    }
    eprintln!("{}", provenance());

    let mut measurements = Vec::new();
    for features in SIZES {
        let pool = pool_size(features);
        let batch = batch_size(features, pool);
        let fixture = Fixture::new(features, pool);
        eprintln!("{features} features: pool {pool}, batch {batch}, {SAMPLES} timed batches");
        for (bounds_name, bounds) in [
            ("none", Bounds::none()),
            ("sutton safety", Bounds::sutton_safety()),
        ] {
            let config = Config::new(INITIAL_STEP_SIZE, META_STEP_SIZE)
                .and_then(|c| c.with_bounds(bounds))
                .map_err(|e| e.to_string())?;
            for operation in [Operation::Predict, Operation::Update] {
                let nanos = time(operation, features, config, &fixture, batch)?;
                let measurement = Measurement {
                    features,
                    bounds: bounds_name,
                    operation,
                    batch,
                    nanos,
                };
                eprintln!(
                    "  {:<7} bounds {bounds_name:<13} median {} ns/call",
                    operation.name(),
                    measurement
                        .median()
                        .map_or_else(|| "-".to_owned(), format_nanos)
                );
                measurements.push(measurement);
            }
        }
    }

    let environment = Environment::detect();
    write(
        options.out(),
        "benchmarks.md",
        &document(&measurements, &environment),
    )
}

/// Examples held in memory. Large enough that a batch never repeats one, small enough
/// that 10,000-feature examples still fit comfortably.
fn pool_size(features: usize) -> usize {
    (4_000_000 / features).clamp(256, 4_096)
}

/// Calls per timed batch. Keeping it at or below the pool size means an update batch
/// never sees the same example twice, so the learner cannot memorise the pool and
/// drive its residuals into the subnormal range.
fn batch_size(features: usize, pool: usize) -> usize {
    let target = (2_000_000 / features).clamp(100, 4_096);
    target.min(pool)
}

fn time(
    operation: Operation,
    features: usize,
    config: Config,
    fixture: &Fixture,
    batch: usize,
) -> Result<Vec<f64>, String> {
    let mut nanos = Vec::with_capacity(SAMPLES);
    for round in 0..WARMUP_BATCHES + SAMPLES {
        let mut learner = Idbd::new(features, config).map_err(|e| e.to_string())?;
        if operation == Operation::Predict {
            prime(&mut learner, fixture, batch)?;
        }
        let started = Instant::now();
        match operation {
            Operation::Predict => {
                for x in fixture.features.iter().take(batch) {
                    let prediction = learner.predict(black_box(x));
                    black_box(prediction.map_err(|e| e.to_string())?);
                }
            }
            Operation::Update => {
                for (x, &target) in fixture.features.iter().zip(&fixture.targets).take(batch) {
                    let report = learner.update(black_box(x), black_box(target));
                    black_box(report.map_err(|e| e.to_string())?);
                }
            }
        }
        let elapsed = started.elapsed().as_secs_f64() * 1e9;
        if round >= WARMUP_BATCHES {
            nanos.push(elapsed / f64::from(u32::try_from(batch).unwrap_or(u32::MAX)));
        }
    }
    nanos.sort_by(|a, b| a.partial_cmp(b).expect("timings are finite"));
    Ok(nanos)
}

/// Learns a few examples so the measured predictions run against nonzero weights.
fn prime(learner: &mut Idbd, fixture: &Fixture, batch: usize) -> Result<(), String> {
    for (x, &target) in fixture
        .features
        .iter()
        .zip(&fixture.targets)
        .take(batch.min(64))
    {
        learner.update(x, target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Machine, operating system, compiler and build settings, as reported by the host.
#[derive(Debug)]
struct Environment {
    cpu: String,
    cores: String,
    os: String,
    rustc: String,
    flags: String,
}

impl Environment {
    fn detect() -> Self {
        Self {
            cpu: shell("sysctl", &["-n", "machdep.cpu.brand_string"]),
            cores: shell("sysctl", &["-n", "hw.ncpu"]),
            os: shell("uname", &["-srm"]),
            rustc: shell("rustc", &["-vV"])
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(", "),
            flags: std::env::var("RUSTFLAGS").unwrap_or_else(|_| "unset".to_owned()),
        }
    }
}

fn shell(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |text| text.trim().to_owned())
}

fn document(measurements: &[Measurement], environment: &Environment) -> String {
    [
        header(environment),
        method_section(),
        results_section(measurements),
        scope_section(),
    ]
    .concat()
}

fn header(environment: &Environment) -> String {
    format!(
        "# Measured core performance\n\n\
         Scenario C24. Produced by `cargo run --release -p idbd-lab --bin bench`.\n\n\
         {provenance}\n\n\
         ## Machine and build\n\n\
         | item | value |\n| --- | --- |\n\
         | CPU | {cpu} |\n\
         | Logical cores | {cores} |\n\
         | OS | {os} |\n\
         | Compiler | {rustc} |\n\
         | Profile | release, `debug = \"line-tables-only\"`, default `opt-level = 3`, \
         `codegen-units = 16`, no LTO |\n\
         | `RUSTFLAGS` | {flags} |\n\n",
        provenance = provenance(),
        cpu = environment.cpu,
        cores = environment.cores,
        os = environment.os,
        rustc = environment.rustc,
        flags = environment.flags,
    )
}

fn method_section() -> String {
    format!(
        "## Method\n\n\
         Timing uses `std::time::Instant` and `std::hint::black_box`, with no benchmark \
         framework.\n\n\
         One configuration is one input size, one bounds setting and one operation. It runs \
         {WARMUP_BATCHES} untimed batches and then {SAMPLES} timed batches. A batch calls the \
         operation once per example over a preallocated pool, and the batch's elapsed time \
         divided by its call count is one sample. The tables report the median of those samples \
         and the 10th and 90th percentiles.\n\n\
         Every batch starts from a freshly constructed learner, so an update batch always begins \
         at zero weights and traces. A batch never reuses an example, which keeps the learner \
         from fitting the pool exactly and driving its residuals into the subnormal range, where \
         arithmetic can take a different amount of time.\n\n\
         The fixture draws each input from a standard normal scaled by `1/sqrt(n)`, so the \
         squared input norm stays near one at every size and one step size ({INITIAL_STEP_SIZE} \
         with meta step size {META_STEP_SIZE}) is stable for all of them. The target is the sum \
         of the first {RELEVANT} inputs with random signs. The fixture is drawn from seed \
         {FIXTURE_SEED}, so the same build measures the same work.\n\n\
         Construction, fixture generation and priming are outside every timed region. Allocation \
         behaviour is covered by scenario C23, not here.\n\n"
    )
}

fn results_section(measurements: &[Measurement]) -> String {
    let mut rows = vec![
        "## Results\n\n\
         | features | bounds | operation | median ns/call | p10 | p90 | ns per feature | \
         calls per batch |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n"
            .to_owned(),
    ];
    for measurement in measurements {
        let (low, high) = measurement.spread().map_or_else(
            || ("-".to_owned(), "-".to_owned()),
            |(low, high)| (format_nanos(low), format_nanos(high)),
        );
        rows.push(format!(
            "| {} | {} | {} | {} | {low} | {high} | {} | {} |\n",
            measurement.features,
            measurement.bounds,
            measurement.operation.name(),
            measurement
                .median()
                .map_or_else(|| "-".to_owned(), format_nanos),
            measurement
                .per_feature()
                .map_or_else(|| "-".to_owned(), |v| format!("{v:.3}")),
            measurement.batch,
        ));
    }
    rows.push(
        "\nThe spread is batch-to-batch variation on a machine running other work, not a \
         confidence interval for the operation.\n\n"
            .to_owned(),
    );
    rows.concat()
}

/// Formats a nanosecond timing with enough digits to compare rows and no more.
fn format_nanos(value: f64) -> String {
    if value < 100.0 {
        format!("{value:.2}")
    } else if value < 10_000.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

fn scope_section() -> String {
    "## What is not in these numbers\n\n\
     These are native timings of the Rust core on one machine. They do not include the \
     WebAssembly binding, the JavaScript call overhead, data marshalling across that boundary, \
     or any browser rendering. The demo's cost is a separate measurement and is not reported \
     here.\n\n\
     Dense `f64` inputs only. A sparse representation would change both the per-call and the \
     per-feature cost.\n"
        .to_owned()
}
